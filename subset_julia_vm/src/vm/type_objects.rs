//! Runtime type-object view backed by the shared Julia type core.
//!
//! `Value::DataType(JuliaType)` remains the compact VM value projection.  This
//! module is the registry/read model used by reflection code when it needs
//! structured `DataType`, `UnionAll`, and `TypeVar` metadata instead of local
//! string parsing.

use crate::inference_core::{CorePrimitive, CoreType, CoreValueParam};
use crate::types::JuliaType;

use super::value::ValueType;
use super::{AbstractTypeDefInfo, RuntimeCompileContext, StructDefInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RuntimeTypeObjectKind {
    DataType,
    UnionAll,
    TypeVar,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RuntimeTypeIdentity {
    kind: RuntimeTypeObjectKind,
    semantic: CoreType,
}

impl RuntimeTypeIdentity {
    pub(crate) fn kind(&self) -> RuntimeTypeObjectKind {
        self.kind
    }

    pub(crate) fn stable_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeTypeHandle {
    identity: RuntimeTypeIdentity,
    projection: JuliaType,
}

impl RuntimeTypeHandle {
    pub(crate) fn identity(&self) -> &RuntimeTypeIdentity {
        &self.identity
    }

    fn core(&self) -> &CoreType {
        &self.identity.semantic
    }

    fn projection(&self) -> &JuliaType {
        &self.projection
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeTypeLayout {
    pub(crate) size_bytes: Option<usize>,
    /// Byte alignment of the type when stored inline as a field or array
    /// element. Mirrors upstream `datatype_alignment` (`base/runtime_internals.jl`):
    /// for a primitive type this is its `sizeof` (clamped to a 1-byte minimum so
    /// zero-size singletons like `Nothing` align to 1); for an immutable struct
    /// it is the maximum alignment over its inline-stored fields (Issue #5107).
    pub(crate) align_bytes: Option<usize>,
    pub(crate) is_bits: bool,
    pub(crate) nfields: usize,
    pub(crate) field_offsets: Option<Vec<usize>>,
}

pub(crate) struct RuntimeTypeRegistry<'a> {
    compile_context: Option<&'a RuntimeCompileContext>,
    abstract_types: &'a [AbstractTypeDefInfo],
    struct_defs: &'a [StructDefInfo],
}

impl<'a> RuntimeTypeRegistry<'a> {
    pub(crate) fn new(
        compile_context: Option<&'a RuntimeCompileContext>,
        abstract_types: &'a [AbstractTypeDefInfo],
    ) -> Self {
        let struct_defs = compile_context.map_or(&[][..], |ctx| ctx.struct_defs.as_slice());
        Self::new_with_struct_defs(compile_context, abstract_types, struct_defs)
    }

    pub(crate) fn new_with_struct_defs(
        compile_context: Option<&'a RuntimeCompileContext>,
        abstract_types: &'a [AbstractTypeDefInfo],
        struct_defs: &'a [StructDefInfo],
    ) -> Self {
        Self {
            compile_context,
            abstract_types,
            struct_defs,
        }
    }

    pub(crate) fn handle(&self, ty: &JuliaType) -> RuntimeTypeHandle {
        let core = CoreType::from(ty);
        let kind = self.kind_for(ty, &core);
        RuntimeTypeHandle {
            identity: RuntimeTypeIdentity {
                kind,
                semantic: core,
            },
            projection: ty.clone(),
        }
    }

    pub(crate) fn object(&'a self, ty: &JuliaType) -> RuntimeTypeObject<'a> {
        RuntimeTypeObject {
            handle: self.handle(ty),
            registry: self,
        }
    }

    pub(crate) fn supertype_name(&self, type_name: &str) -> String {
        if let Some(parent) = dense_array_alias_supertype_name(type_name) {
            return parent;
        }

        // For a user type, resolve the declared parent of the *base* type. A
        // parametric instantiation like `Box{Int64}` shares the base `Box`'s
        // declared parent; substituting the concrete arguments into a
        // parametric parent (`AbsB{T}` -> `AbsB{Int64}`) keeps `supertype`
        // consistent with upstream Julia. (Issue #3909)
        let (base_name, args) = match split_parametric_name(type_name) {
            Some((base, args)) => (base, args),
            None => (type_name.to_string(), Vec::new()),
        };

        // Parametric struct schemas live in the compile context keyed by base
        // name; their declared parent carries the bound type vars (`AbsB{T}`),
        // so they must be consulted before the monomorphized `struct_defs`
        // entries when a parametric instantiation is being resolved. This is
        // consulted BEFORE the builtin direct-supertype table because some
        // pure-Julia Base structs are BOTH a builtin name and a declared struct:
        // `struct Pairs{K,V,I,A} <: AbstractDict{K,V}` is hardcoded to `Any` in
        // that table, which would drop the threaded `AbstractDict{K,V}` parent
        // (Issue #5882). Gated on a PARAMETRIC instantiation (`args` non-empty) so a
        // BARE name keeps the builtin-table parent — e.g. `supertype(Dict)` stays the
        // bare `AbstractDict`, not `AbstractDict{K, V}` — while an instantiation like
        // `Pairs{Symbol,Int64,...}` threads its concrete arguments through.
        if let Some(ctx) = self.compile_context {
            if let Some(p) = ctx.parametric_structs.get(&base_name) {
                if !args.is_empty() {
                    let names: Vec<String> =
                        p.def.type_params.iter().map(|tp| tp.name.clone()).collect();
                    return substitute_parent_name(p.def.parent_type.as_deref(), &names, &args);
                } else if let Some(parent) = p.def.parent_type.as_deref() {
                    // Bare parametric name (`supertype(RotMatrix)`): return the
                    // bare parent family (`Rotation`) rather than `Any` (#8092).
                    // Stripping the parent's parameters keeps the bare-name form
                    // (mirrors the args-gate that kept `supertype(Dict)` from
                    // over-parameterising to `AbstractDict{K, V}`).
                    let family = parent.split('{').next().unwrap_or(parent);
                    return family.to_string();
                }
            }
        }

        if let Some(parent) = CoreType::direct_builtin_supertype_name_for_julia_name(type_name) {
            return parent.to_string();
        }

        // A user-declared primitive type (`primitive type MyU8 <: Unsigned 8 end`,
        // Issue #5058) resolves to its declared abstract supertype (default `Any`).
        if let Some(ctx) = self.compile_context {
            if let Some(pt) = ctx.primitive_types.iter().find(|p| p.name == base_name) {
                return substitute_parent_name(pt.parent.as_deref(), &[], &args);
            }
        }

        let (parent, type_params): (Option<String>, &[String]) =
            if let Some(def) = self.struct_defs.iter().find(|d| d.name == base_name) {
                (def.parent_type.clone(), &[])
            } else if let Some(def) = self.abstract_types.iter().find(|d| d.name == base_name) {
                (def.parent.clone(), def.type_params.as_slice())
            } else {
                (None, &[])
            };

        substitute_parent_name(parent.as_deref(), type_params, &args)
    }

    /// Return the direct subtypes of the named abstract type, matching upstream
    /// `InteractiveUtils.subtypes(T)` as closely as the registry allows.
    ///
    /// The result merges three sources — the builtin type lattice
    /// (`Integer` -> `Bool`/`Signed`/`Unsigned`), user `struct` definitions, and
    /// user `abstract type` definitions — then deduplicates and sorts by the
    /// type's string name. Upstream sorts via
    /// `permute!(sts, sortperm(map(string, sts)))`; we reproduce that ordering so
    /// fixtures can assert a stable, comparable list. (Issue #5057)
    ///
    /// Three correctness rules keep the list upstream-faithful:
    /// 1. **Base abstract types live in both the builtin lattice and the runtime
    ///    `abstract_types` registry** (e.g. `Signed`/`Unsigned` under `Integer`).
    ///    Deduplication by name collapses the overlap so each child appears once.
    /// 2. **Parametric structs are reported by their base `UnionAll` name**
    ///    (`Box`, not the monomorphized `Box{Int64}`/`Box{Float64}`), mirroring
    ///    `subtypes(AbsB) == Any[Box, Plain]`. Monomorphized instantiations carry
    ///    a `{...}` suffix and are filtered out of `struct_defs`; the base name is
    ///    recovered from the `parametric_structs` schema instead.
    /// 3. **Parents may be parametric** (`AbsB{T}`); matching compares base names.
    pub(crate) fn direct_subtypes(&self, type_name: &str) -> Vec<JuliaType> {
        let parent_base = base_name_without_params(type_name);

        let mut names: Vec<String> =
            CoreType::direct_builtin_subtype_names_for_julia_name(type_name)
                .unwrap_or_default()
                .into_iter()
                .map(str::to_string)
                .collect();

        // Base parametric struct names (`Box` from `struct Box{T} <: AbsB`).
        if let Some(ctx) = self.compile_context {
            for (base, schema) in &ctx.parametric_structs {
                if parent_base_matches(schema.def.parent_type.as_deref(), parent_base) {
                    names.push(base.clone());
                }
            }
        }

        // Non-parametric user structs. Monomorphized parametric instantiations
        // also appear here with a `{...}` suffix; they are excluded because the
        // base parametric name is supplied above (matching upstream, which lists
        // the `UnionAll`, not each instantiation).
        names.extend(
            self.struct_defs
                .iter()
                .filter(|def| !def.name.contains('{'))
                .filter(|def| parent_base_matches(def.parent_type.as_deref(), parent_base))
                .map(|def| def.name.clone()),
        );

        // User abstract types.
        names.extend(
            self.abstract_types
                .iter()
                .filter(|def| parent_base_matches(def.parent.as_deref(), parent_base))
                .map(|def| def.name.clone()),
        );

        // Deduplicate (builtin lattice vs. registry overlap) then sort by string
        // name to match upstream's `sortperm(map(string, sts))`.
        names.sort_unstable();
        names.dedup();

        names
            .into_iter()
            .map(|name| JuliaType::from_name_or_struct(&name))
            .collect()
    }

    fn kind_for(&self, ty: &JuliaType, core: &CoreType) -> RuntimeTypeObjectKind {
        if matches!(core, &CoreType::TypeVar(_)) {
            // A single uppercase-letter name (`P`, `T`, `N`, ...) is parsed into a
            // `CoreType::TypeVar` by the string-level type-variable heuristic. But
            // when that name is a *declared* struct / abstract type, it names a
            // real type object and must classify as a `DataType` (or `UnionAll`
            // for parametric / unionall-like declarations) regardless of length —
            // only an *undefined* single-letter name in a `where` / `{...}`
            // parameter position is a genuine type variable (Issue #5252).
            if self.declares_type_name(ty) {
                return if self.is_unionall_like(ty) {
                    RuntimeTypeObjectKind::UnionAll
                } else {
                    RuntimeTypeObjectKind::DataType
                };
            }
            return RuntimeTypeObjectKind::TypeVar;
        }

        if self.is_unionall_like(ty) {
            RuntimeTypeObjectKind::UnionAll
        } else {
            RuntimeTypeObjectKind::DataType
        }
    }

    /// Whether `ty` names a user-declared concrete struct or abstract type that
    /// happens to collide with the single-uppercase-letter type-variable
    /// spelling (`struct P ... end`, `abstract type T end`). The lookup keys off
    /// the type's *base* name so that both the bare declaration (`P`) and any
    /// parametric instantiation (`P{Int}`) are recognised.
    ///
    /// Only bare/parametric *named* types are considered: structured projections
    /// (tuples, unions, `Vector{T}`, an explicit `JuliaType::TypeVar`, ...) are
    /// never user-declared type names and must keep their genuine classification
    /// so legitimate `where T` / `Vector{T}` type variables are unaffected
    /// (Issue #5252).
    fn declares_type_name(&self, ty: &JuliaType) -> bool {
        let name = match ty {
            JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => name.clone(),
            _ => return false,
        };
        let base = parametric_base_name(&name).unwrap_or(name);
        self.declares_base_name(&base)
    }

    /// Like [`declares_type_name`], but only matches a *bare* type name with no
    /// `{...}` parameters. Used by the concreteness check, where a parametric
    /// instantiation (`Box{T}`) must still be scrutinised for genuinely free
    /// type-variable arguments while a bare declared name (`P`) that merely
    /// collides with the type-variable spelling is treated as a real type
    /// (Issue #5252).
    fn declares_bare_type_name(&self, ty: &JuliaType) -> bool {
        let name = match ty {
            JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => name,
            _ => return false,
        };
        if name.contains('{') {
            return false;
        }
        self.declares_base_name(name)
    }

    /// Whether `base` (a name already stripped of any `{...}` parameters) is a
    /// declared user struct, abstract type, or parametric struct schema.
    fn declares_base_name(&self, base: &str) -> bool {
        self.struct_defs
            .iter()
            .any(|def| declared_name_matches_base(&def.name, base))
            || self
                .abstract_types
                .iter()
                .any(|def| declared_name_matches_base(&def.name, base))
            || self.has_parametric_schema(base)
    }

    fn is_unionall_like(&self, ty: &JuliaType) -> bool {
        match ty {
            JuliaType::UnionAll { .. } | JuliaType::Array | JuliaType::Dict | JuliaType::Set => {
                true
            }
            JuliaType::Struct(name) if !name.contains('{') => {
                matches!(
                    name.as_str(),
                    "Vector" | "Matrix" | "DenseArray" | "DenseVector" | "DenseMatrix"
                ) || is_bare_ref_family_name(name)
                    || self.has_parametric_schema(name)
            }
            _ => false,
        }
    }

    fn has_parametric_schema(&self, name: &str) -> bool {
        self.compile_context
            .and_then(|ctx| ctx.parametric_structs.get(name))
            .is_some()
            || self
                .abstract_types
                .iter()
                .any(|def| def.name == name && !def.type_params.is_empty())
    }
}

pub(crate) struct RuntimeTypeObject<'a> {
    handle: RuntimeTypeHandle,
    registry: &'a RuntimeTypeRegistry<'a>,
}

impl RuntimeTypeObject<'_> {
    pub(crate) fn identity(&self) -> &RuntimeTypeIdentity {
        self.handle.identity()
    }

    pub(crate) fn kind(&self) -> RuntimeTypeObjectKind {
        self.identity().kind()
    }

    pub(crate) fn runtime_type_projection(&self) -> JuliaType {
        match self.kind() {
            RuntimeTypeObjectKind::DataType => JuliaType::DataType,
            RuntimeTypeObjectKind::UnionAll => JuliaType::Struct("UnionAll".to_string()),
            RuntimeTypeObjectKind::TypeVar => JuliaType::Struct("TypeVar".to_string()),
        }
    }

    pub(crate) fn layout(&self) -> RuntimeTypeLayout {
        if let Some(def) = self.user_struct_def() {
            return RuntimeTypeLayout {
                size_bytes: def.layout_size_bytes(self.registry.struct_defs),
                align_bytes: def.layout_align_bytes(self.registry.struct_defs),
                is_bits: def.is_isbits_with_struct_defs(self.registry.struct_defs),
                nfields: def.fields.len(),
                field_offsets: def.field_offsets_bytes(self.registry.struct_defs),
            };
        }

        // A user-declared primitive type (`primitive type Name Bits end`,
        // Issue #5058) has a fixed `sizeof == bits / 8`, no fields, and is an
        // isbits leaf — mirroring how upstream lays out a primitive `DataType`.
        if let Some(pt) = self.primitive_def() {
            let size_bytes = (pt.bits / 8) as usize;
            return RuntimeTypeLayout {
                size_bytes: Some(size_bytes),
                align_bytes: Some(size_bytes.max(1)),
                is_bits: true,
                nfields: 0,
                field_offsets: Some(Vec::new()),
            };
        }

        // A parametric instantiation whose monomorphized def is not present
        // still has a definite field count from its schema. Byte-level layout
        // is left unknown here since it depends on the concrete arguments.
        if let Some(schema) = self.parametric_schema() {
            return RuntimeTypeLayout {
                size_bytes: None,
                align_bytes: None,
                is_bits: false,
                nfields: schema.def.fields.len(),
                field_offsets: None,
            };
        }

        let field_count = self
            .handle
            .core()
            .builtin_field_metadata()
            .map_or(0, |fields| fields.len());
        let size_bytes = self.handle.core().builtin_sizeof_bytes();
        RuntimeTypeLayout {
            size_bytes,
            // For a builtin primitive `DataType` upstream's `datatype_alignment`
            // equals its `sizeof`, clamped to a 1-byte minimum so zero-size
            // singletons (`Nothing`/`Missing`) still align to 1 (Issue #5107).
            align_bytes: size_bytes.map(|size| size.max(1)),
            is_bits: self.handle.core().is_builtin_bits_type(),
            nfields: field_count,
            field_offsets: self.builtin_field_offsets(),
        }
    }

    pub(crate) fn size_bytes(&self) -> Option<usize> {
        self.layout().size_bytes
    }

    /// Byte alignment used when an instance of this type is stored inline as a
    /// struct field or array element. Mirrors upstream `datatype_alignment`
    /// (`base/runtime_internals.jl`) for the immutable/isbits cases this VM
    /// supports (Issue #5107).
    pub(crate) fn alignment_bytes(&self) -> Option<usize> {
        self.layout().align_bytes
    }

    /// Whether instances of this type are stored inline (unboxed) when held in a
    /// container, mirroring upstream `Base.allocatedinline` /
    /// `jl_stored_inline` (`julia/src/datatype.c`). For the subset this VM
    /// supports the rule reduces to: the type must be a concrete, immutable
    /// `DataType`. Concrete immutable structs are inline even when they carry
    /// boxed fields (e.g. a `String` field), matching upstream; mutable structs,
    /// abstract types, `UnionAll`s, and variable-size builtins such as `String`
    /// and `Symbol` (which the VM models as mutable) are boxed (Issue #5107).
    pub(crate) fn is_stored_inline(&self) -> bool {
        self.is_concrete_type() && !self.is_mutable_type()
    }

    pub(crate) fn is_bits_type(&self) -> bool {
        // Upstream Julia: `isbitstype(T) === isa(T, DataType) && (flags bit set)`,
        // where the runtime sets that flag iff `T` is a concrete, immutable
        // `DataType` whose every field type is itself isbits (a primitive
        // numeric/`Bool`/`Char`/`Nothing`/`Missing`, or an immutable struct of
        // isbits fields — recursively). `String`/`Symbol`/`BigInt`/`BigFloat`,
        // arrays, mutable structs, abstract types, and `UnionAll`s are NOT
        // isbits. See `isbitstype(@nospecialize t)` in
        // `julia/base/runtime_internals.jl` and `jl_compute_field_offsets` in
        // `julia/src/datatype.c`. (Issue #5104)
        self.projection_is_bits(self.handle.projection())
    }

    /// Recursive isbits check on a `JuliaType` projection, mirroring the
    /// concreteness recursion in [`projection_is_concrete`]. Field/element
    /// sub-projections are resolved through the same registry so nested
    /// immutable structs and parametric instantiations (`Box{Int}`) are
    /// classified by their *concrete* field types rather than their declared
    /// type variables.
    fn projection_is_bits(&self, ty: &JuliaType) -> bool {
        match ty {
            // `Union{...}`, `Union{}` (bottom), free type variables, `Type{T}`
            // kinds, and `UnionAll` aliases are not concrete `DataType`s and so
            // are never isbits.
            JuliaType::Union(_)
            | JuliaType::Bottom
            | JuliaType::TypeVar(..)
            | JuliaType::TypeOf(_) => false,

            // Bare `Tuple` is the abstract `Tuple{Vararg{Any}}`: not isbits.
            JuliaType::Tuple => false,

            // `Tuple{...}` is isbits iff every element type is isbits. The empty
            // tuple `Tuple{}` is therefore isbits (vacuously true), matching
            // upstream. `Vararg`/`UnionAll` elements project to non-`DataType`
            // sub-objects and are rejected by the recursive element check.
            JuliaType::TupleOf(elements) => {
                elements.iter().all(|element| self.element_is_bits(element))
            }

            // Any other projection is a (possibly parametric) `DataType`,
            // classified through the registry-resolved field types of
            // `self`'s projection.
            _ => self.datatype_is_bits(),
        }
    }

    /// isbits classification for a concrete-or-not `DataType` projection (the
    /// non-tuple arm of [`projection_is_bits`]), operating on `self`'s own
    /// projection.
    fn datatype_is_bits(&self) -> bool {
        // A `DataType` must be concrete and immutable to be isbits.
        if !self.is_concrete_type() || self.is_mutable_type() {
            return false;
        }

        // A user-declared primitive type (`primitive type Name Bits end`,
        // Issue #5058) is an isbits leaf with no fields, like a builtin primitive.
        if self.primitive_def().is_some() {
            return true;
        }

        // Builtin (non-user, non-tuple) `DataType`s are leaves: only the fixed
        // primitive/`Bool`/`Char`/`Nothing`/`Missing` allowlist is isbits.
        // `String`/`Symbol`/`BigInt`/`BigFloat`/array types are concrete (and
        // some immutable) but deliberately excluded by this allowlist, matching
        // upstream. User structs and builtin parametric wrappers (`Complex`,
        // `Rational`) carry field metadata and are handled by the recursion
        // below.
        let has_user_or_parametric_fields = self.user_struct_def().is_some()
            || self.parametric_schema().is_some()
            || self
                .handle
                .core()
                .builtin_field_metadata()
                .is_some_and(|f| !f.is_empty());
        if !has_user_or_parametric_fields {
            return self.handle.core().is_builtin_bits_type();
        }

        // Concrete immutable struct (or builtin wrapper like `Complex{T}`):
        // isbits iff every concrete field type is isbits, resolved recursively
        // through the registry so nested structs and substituted type
        // parameters are honoured.
        self.field_types()
            .is_some_and(|fields| fields.iter().all(|f| self.element_is_bits(f)))
    }

    /// Whether a tuple element / struct field type is isbits, resolved through
    /// the registry so nested structs recurse with the same rule.
    fn element_is_bits(&self, element: &JuliaType) -> bool {
        // An unbounded `Vararg` tail has no concrete layout, so a tuple
        // containing it is not isbits. Bounded `Vararg{T,N}` is expanded into
        // individual element projections before reaching here.
        if matches!(
            CoreType::from(element),
            CoreType::Vararg(_) | CoreType::VarargLen { .. }
        ) {
            return false;
        }
        self.registry.object(element).is_bits_type()
    }

    pub(crate) fn field_names(&self) -> Option<Vec<String>> {
        if let Some(fields) = self.handle.core().builtin_field_metadata() {
            return Some(
                fields
                    .into_iter()
                    .map(|(name, _)| name.to_string())
                    .collect(),
            );
        }

        if let Some(def) = self.user_struct_def() {
            return Some(def.fields.iter().map(|(name, _)| name.clone()).collect());
        }

        // Parametric schema (`Box{Int64}` / bare `Box`): the registered
        // `StructDefInfo` may only exist for monomorphized instances, so fall
        // back to the parametric definition. Field names do not depend on the
        // type arguments, matching upstream Julia.
        self.parametric_schema()
            .map(|def| def.def.fields.iter().map(|f| f.name.clone()).collect())
    }

    pub(crate) fn field_offset(&self, index: usize) -> Option<usize> {
        if index == 0 {
            return None;
        }
        self.layout()
            .field_offsets
            .and_then(|offsets| offsets.get(index - 1).copied())
    }

    pub(crate) fn builtin_field_names(&self) -> Option<Vec<String>> {
        self.handle.core().builtin_field_metadata().map(|fields| {
            fields
                .into_iter()
                .map(|(name, _)| name.to_string())
                .collect()
        })
    }

    pub(crate) fn field_types(&self) -> Option<Vec<JuliaType>> {
        // `Tuple{A,B}` fields are its positional element types, matching
        // upstream `fieldtypes(Tuple{A,B}) == (A, B)` (Issue #8385).
        if let JuliaType::TupleOf(elements) = self.handle.projection() {
            return Some(elements.clone());
        }

        // `Pair{A,B}` is represented as a NON-parametric `Pair` struct in sjulia
        // (base/pair.jl), so its declared field types are untyped (`Any`). Resolve
        // `fieldtypes`/`fieldtype` from the concrete type arguments instead:
        // `first::A`, `second::B`, matching upstream `fieldtypes(Pair{Int,String}) ==
        // (Int64, String)` (Issue #5733).
        {
            let proj_name = self.handle.projection().name();
            if let Some((base, args)) = split_parametric_name(proj_name.as_ref()) {
                if base == "Pair" && args.len() == 2 {
                    return Some(args);
                }
            }
        }

        if let Some(fields) = self.handle.core().builtin_field_metadata() {
            return Some(
                fields
                    .into_iter()
                    .filter_map(|(_, field_type)| core_type_to_reflection_julia_type(&field_type))
                    .collect(),
            );
        }

        // Parametric instantiation (`Box{Int64}`): substitute declared type
        // parameters with the concrete arguments so `fieldtypes` matches upstream.
        if let Some(field_types) = self.parametric_field_types() {
            return Some(field_types);
        }

        if let Some(def) = self.user_struct_def() {
            return Some(if def.field_julia_types.len() == def.fields.len() {
                def.field_julia_types.clone()
            } else {
                def.fields
                    .iter()
                    .map(|(_, field_type)| {
                        value_type_to_reflection_julia_type(field_type, self.registry.struct_defs)
                    })
                    .collect()
            });
        }

        // Bare parametric type (un-instantiated `UnionAll` such as `Box`): no
        // concrete arguments are available, so each declared field type is its
        // type variable's upper bound (`Any` when unbounded), matching upstream
        // `fieldtype(Box, 1) === Any` / `fieldtypes(Box) === (Any,)` (Issue #5099).
        self.bare_parametric_field_types()
    }

    /// Field `JuliaType`s for a *bare* parametric struct (`Box`, not `Box{Int}`),
    /// substituting each declared type variable with its upper bound (defaulting
    /// to `Any`). This keeps `fieldtypes`/`fieldtype` consistent with the
    /// `fieldnames`/`fieldcount` reflection paths, which already resolve the bare
    /// `UnionAll` via `parametric_schema`.
    fn bare_parametric_field_types(&self) -> Option<Vec<JuliaType>> {
        let def = self.parametric_schema()?;
        if def.def.type_params.is_empty() {
            return None;
        }
        // type-parameter name -> upper-bound type (Any when unbounded).
        let bounds: std::collections::HashMap<&str, JuliaType> = def
            .def
            .type_params
            .iter()
            .map(|tp| {
                let bound = tp
                    .upper_bound
                    .as_deref()
                    .and_then(JuliaType::from_name)
                    .unwrap_or(JuliaType::Any);
                (tp.name.as_str(), bound)
            })
            .collect();
        let subst: std::collections::HashMap<&str, &JuliaType> =
            bounds.iter().map(|(k, v)| (*k, v)).collect();
        Some(
            def.def
                .fields
                .iter()
                .map(|field| match &field.type_expr {
                    Some(expr) => expr.substitute_to_julia_type_lossy(&subst),
                    None => JuliaType::Any,
                })
                .collect(),
        )
    }

    pub(crate) fn builtin_field_types(&self) -> Option<Vec<JuliaType>> {
        self.handle.core().builtin_field_metadata().map(|fields| {
            fields
                .into_iter()
                .filter_map(|(_, field_type)| core_type_to_reflection_julia_type(&field_type))
                .collect()
        })
    }

    /// Full parameter list for the `.parameters` reflection path, including
    /// integer/value parameters (e.g. the array dimensionality `N`) that
    /// `parameters()` drops because it only models type parameters as
    /// `JuliaType` (Issue #5162).
    ///
    /// Upstream Julia: `Array{T,N}.parameters == svec(T, N)`,
    /// `Vector{Int}.parameters == svec(Int64, 1)`,
    /// `Matrix{Float64}.parameters == svec(Float64, 2)`,
    /// `Val{5}.parameters == svec(5)`.
    ///
    /// `Vector`/`Matrix` are stored as aliases that only carry the element
    /// type, so the dimensionality value parameter (`1` / `2`) is injected
    /// here to mirror their `Array{T, N}` expansion.
    pub(crate) fn parameters_with_values(&self) -> Vec<ReflectionParameter> {
        let core = self.handle.core();

        // Vector{T} / Matrix{T} are aliases for Array{T, 1} / Array{T, 2}; the
        // dimensionality value parameter is not stored on the core type, so
        // append it to match upstream `.parameters`.
        if let CoreType::Struct { name, params } = core {
            let alias_dim = match name.as_str() {
                "Vector" | "DenseVector" => Some(1),
                "Matrix" | "DenseMatrix" => Some(2),
                _ => None,
            };
            if let Some(dim) = alias_dim {
                if params.len() == 1 {
                    let mut out: Vec<ReflectionParameter> = params
                        .iter()
                        .filter_map(reflection_parameter_from_core)
                        .collect();
                    out.push(ReflectionParameter::Int(dim));
                    return out;
                }
            }
        }

        core.type_parameters()
            .iter()
            .filter_map(reflection_parameter_from_core)
            .collect()
    }

    pub(crate) fn unionall_var(&self) -> Option<JuliaType> {
        match self.handle.projection() {
            JuliaType::UnionAll { var, bound, .. } => Some(JuliaType::TypeVar(
                var.clone(),
                bound.as_ref().map(|b| (**b).clone()),
            )),
            JuliaType::Array => Some(JuliaType::TypeVar("T".to_string(), None)),
            JuliaType::Dict => Some(JuliaType::TypeVar("K".to_string(), None)),
            JuliaType::Set => Some(JuliaType::TypeVar("T".to_string(), None)),
            JuliaType::Struct(name) => {
                if name.contains('{') {
                    return None;
                }
                match name.as_str() {
                    "Vector" | "Matrix" | "DenseArray" | "DenseVector" | "DenseMatrix" => {
                        Some(JuliaType::TypeVar("T".to_string(), None))
                    }
                    _ => self
                        .parametric_type_params(name)
                        .and_then(|params| params.first().cloned()),
                }
            }
            _ => None,
        }
    }

    pub(crate) fn unionall_body(&self) -> Option<JuliaType> {
        match self.handle.projection() {
            JuliaType::UnionAll { body, .. } => Some(body.as_ref().clone()),
            JuliaType::Array => Some(nested_unionall_body(
                "Array",
                vec![
                    JuliaType::TypeVar("T".to_string(), None),
                    JuliaType::TypeVar("N".to_string(), None),
                ],
            )),
            JuliaType::Dict => Some(nested_unionall_body(
                "Dict",
                vec![
                    JuliaType::TypeVar("K".to_string(), None),
                    JuliaType::TypeVar("V".to_string(), None),
                ],
            )),
            JuliaType::Set => Some(JuliaType::Struct("Set{T}".to_string())),
            JuliaType::Struct(name) => {
                if name.contains('{') {
                    return None;
                }
                match name.as_str() {
                    "Vector" => Some(JuliaType::Struct("Array{T, 1}".to_string())),
                    "Matrix" => Some(JuliaType::Struct("Array{T, 2}".to_string())),
                    "DenseArray" => Some(nested_unionall_body(
                        "DenseArray",
                        vec![
                            JuliaType::TypeVar("T".to_string(), None),
                            JuliaType::TypeVar("N".to_string(), None),
                        ],
                    )),
                    "DenseVector" => Some(JuliaType::Struct("DenseArray{T, 1}".to_string())),
                    "DenseMatrix" => Some(JuliaType::Struct("DenseArray{T, 2}".to_string())),
                    _ => self
                        .parametric_type_params(name)
                        .map(|params| nested_unionall_body(name, params)),
                }
            }
            _ => None,
        }
    }

    pub(crate) fn typevar_name(&self) -> Option<String> {
        match self.handle.core() {
            CoreType::TypeVar(var) => Some(var.name.clone()),
            _ => None,
        }
    }

    /// Canonical `TypeName` symbol for this type, matching upstream
    /// `nameof(::Type)` / `t.name.name` (Issue #5106).
    ///
    /// The result is the *base* name with type parameters and `where` clauses
    /// removed, and Base display aliases collapsed onto the underlying
    /// `TypeName`: `Vector`, `Vector{Int}`, `Matrix`, `Matrix{Int}` and
    /// `Array{Int,1}` all share the `Array` `TypeName`, so each yields
    /// `:Array`. A `TypeVar` reports its own name (matching `nameof` of a
    /// bound variable used by reflection callers).
    pub(crate) fn typename_symbol(&self) -> String {
        if let CoreType::TypeVar(var) = self.handle.core() {
            return var.name.clone();
        }
        let projection = self.handle.projection().name();
        canonical_typename(projection.as_ref())
    }

    pub(crate) fn typevar_lower_bound(&self) -> Option<JuliaType> {
        match self.handle.core() {
            CoreType::TypeVar(var) => Some(
                var.lower_bound
                    .as_deref()
                    .and_then(core_type_to_reflection_julia_type)
                    .unwrap_or(JuliaType::Bottom),
            ),
            _ => None,
        }
    }

    pub(crate) fn typevar_upper_bound(&self) -> Option<JuliaType> {
        match self.handle.core() {
            CoreType::TypeVar(var) => Some(
                var.upper_bound
                    .as_deref()
                    .and_then(core_type_to_reflection_julia_type)
                    .unwrap_or(JuliaType::Any),
            ),
            _ => None,
        }
    }

    /// Mirror of upstream `isabstracttype` (`base/runtime_internals.jl`): the
    /// type is `unwrap_unionall`'d, then a `DataType` is abstract iff its
    /// `TypeName` carries the abstract flag. A `Union`/`TypeVar` is never
    /// abstract.
    ///
    /// Resolving through the *base name* is what supplies the `unwrap_unionall`
    /// behaviour for parametric instantiations: an abstract parametric type such
    /// as `AbsP{Int}` keeps the abstractness of its declaration `AbsP` (the
    /// `abstract_types` registry is keyed by base name), matching upstream
    /// `isabstracttype(AbsP{Int}) == true` (Issue #5102).
    pub(crate) fn is_abstract_type(&self) -> bool {
        let type_name = self.handle.projection().name();
        let base =
            parametric_base_name(type_name.as_ref()).unwrap_or_else(|| type_name.to_string());
        // Upstream `Ref` is `abstract type Ref{T} end`, so both the bare `Ref`
        // UnionAll and every `Ref{T}` instantiation are abstract. The concrete
        // half of the #5130 box (`Base.RefValue`) stays non-abstract
        // (Issue #5223).
        if is_ref_abstract_base_name(&base) {
            return true;
        }
        self.registry
            .abstract_types
            .iter()
            .any(|def| def.name == base)
            || self.handle.core().is_builtin_abstract_datatype()
            || CoreType::is_builtin_abstract_datatype_for_julia_name(&base)
    }

    /// Mirror of upstream Julia's `dt->isconcretetype` flag (`isconcretetype`
    /// in `base/reflection.jl`, computed in `jl_precompute_memoized_dt`):
    ///
    /// A type is concrete iff it is a `DataType` (not a `UnionAll`, `Union`,
    /// `TypeVar`, or `Union{}`), it is not abstract, it has no free type
    /// variables, and — when it is a `Tuple` — every parameter is itself
    /// concrete. This makes concrete parametric instantiations (`Box{Int}`,
    /// `Pair{Int,String}`), function singleton types (`typeof(f)`), and the
    /// empty tuple type (`Tuple{}`) concrete, while keeping `UnionAll`/bare
    /// parametric types (`Box`, `Vector`, `Tuple`), `Union`s, and tuples with
    /// abstract/unbound element types (`Tuple{Number}`, `Tuple{Int,Vector}`)
    /// non-concrete (Issue #5203).
    pub(crate) fn is_concrete_type(&self) -> bool {
        // UnionAll / TypeVar projections (`Box`, `Vector`, bare `Tuple`, a free
        // `T`) can have no instances of a definite layout, so they are never
        // concrete. This also rejects every `UnionAll`-like alias up front.
        if self.kind() != RuntimeTypeObjectKind::DataType {
            return false;
        }
        self.projection_is_concrete(self.handle.projection())
    }

    /// Recursive concreteness check on a `JuliaType` projection, used by
    /// [`is_concrete_type`]. Sub-projections (e.g. tuple element types) are
    /// resolved through the same registry so abstractness / UnionAll-ness of
    /// element types is honoured consistently.
    fn projection_is_concrete(&self, ty: &JuliaType) -> bool {
        match ty {
            // `Union{...}` and `Union{}` (bottom) are not `DataType`s and have
            // no concrete layout.
            JuliaType::Union(_) | JuliaType::Bottom => false,

            // A free type variable is never concrete.
            JuliaType::TypeVar(..) => false,

            // `Type{T}` is a "kind" object, which upstream `isconcretetype`
            // reports as non-concrete (`isconcretetype(Type{Int}) == false`),
            // even though `Type{Int}` has a single instance. Keeping this false
            // matches both upstream and the existing `types_isconcretetype`
            // fixture.
            JuliaType::TypeOf(_) => false,

            // Bare `Tuple` is the "any tuple" `Tuple{Vararg{Any}}`, which
            // upstream marks non-concrete unconditionally
            // (`jl_anytuple_type->isconcretetype = 0`).
            JuliaType::Tuple => false,

            // `Tuple{...}` is concrete iff every element is concrete. The empty
            // tuple `Tuple{}` is therefore concrete (vacuously true), matching
            // upstream. `Vararg`/`UnionAll` elements project to non-DataType
            // sub-objects and are rejected by the recursive call.
            JuliaType::TupleOf(elements) => elements
                .iter()
                .all(|element| self.element_is_concrete(element)),

            // Any other projection that survived the `kind()` gate is a
            // (possibly parametric) `DataType`: concrete iff it is not abstract
            // and carries no *free* type variables. Builtin concrete datatypes,
            // user structs (including parametric instantiations such as
            // `Box{Int}`), and function singleton types (`typeof(f)`) all land
            // here. Note that abstract or `UnionAll`-valued *parameters* (e.g.
            // `Box{Number}`, `Box{Vector}`) do NOT make the type non-concrete —
            // only a free type variable does — matching upstream.
            //
            // A *bare* builtin parametric base written without type arguments
            // (`Pair`, `UnitRange`, `StepRange`, `NamedTuple`) is a `UnionAll`
            // upstream and therefore not concrete, even though sjulia currently
            // projects it onto a `DataType`-kind value. Rejecting it here keeps
            // `isconcretetype(Pair) == false` consistent with upstream without
            // disturbing the `typeof`/`kind` projection (Issue #5102).
            _ => {
                // A *bare* declared type whose name collides with the
                // single-letter type-variable spelling (`struct P ... end`) is a
                // real concrete type, not a free type variable, so do not let the
                // `projection_has_free_typevars` string heuristic reject it. This
                // is restricted to bare names: a parametric instantiation like
                // `Box{T}` must still be rejected for the genuinely free `T`
                // argument (Issue #5252).
                let has_free_typevars =
                    !self.registry.declares_bare_type_name(ty) && projection_has_free_typevars(ty);
                !self.is_abstract_type() && !has_free_typevars && !self.is_bare_builtin_parametric()
            }
        }
    }

    /// Whether a tuple element type is concrete (or `Union{}`, which upstream
    /// permits as a tuple element while keeping the tuple concrete).
    fn element_is_concrete(&self, element: &JuliaType) -> bool {
        if matches!(element, JuliaType::Bottom) {
            return true;
        }
        // An unbounded `Vararg{T}` tail (no concrete length) leaves the tuple
        // length undetermined, so the tuple type is not concrete. A *bounded*
        // `Vararg{T, 3}` is expanded into individual element projections before
        // reaching here, so only the unbounded forms (`Vararg` / `VarargLen`
        // with a free length) need to be rejected.
        if matches!(
            CoreType::from(element),
            CoreType::Vararg(_) | CoreType::VarargLen { .. }
        ) {
            return false;
        }
        self.registry.object(element).is_concrete_type()
    }

    /// Whether this projection is a *bare* builtin parametric base written
    /// without type arguments (`Pair`, `UnitRange`, `StepRange`, `NamedTuple`,
    /// ...). Upstream represents these as `UnionAll`s, so `isconcretetype`
    /// reports `false`; sjulia projects some of them onto `DataType`-kind values
    /// (`typeof(Pair) === DataType`), so the concreteness predicate filters them
    /// explicitly. A type *with* arguments (`Pair{Int,String}`) is excluded
    /// because it is a concrete `DataType` (Issue #5102).
    fn is_bare_builtin_parametric(&self) -> bool {
        let type_name = self.handle.projection().name();
        let type_name = type_name.as_ref();
        // Only the bare form (no `{...}` arguments) is a UnionAll upstream.
        if type_name.contains('{') {
            return false;
        }
        // `Tuple` is the any-tuple `DataType` (handled by the dedicated `Tuple`
        // arm above), not a `UnionAll`; every other builtin parametric base is.
        matches!(
            type_name,
            "Pair"
                | "UnitRange"
                | "StepRange"
                | "StepRangeLen"
                | "LinRange"
                | "OneTo"
                | "NamedTuple"
                | "Rational"
                | "Complex"
                | "Memory"
                | "MemoryRef"
        )
    }

    /// Mirror of upstream `isstructtype` (`base/runtime_internals.jl`):
    /// `unwrap_unionall(t)` must be a `DataType` that is neither a primitive
    /// type nor an abstract type. `Union`/`TypeVar`/`Bottom` are not structs.
    ///
    /// The base name resolves the `unwrap_unionall` behaviour: bare user
    /// parametric structs (`Box`) and their instantiations (`Box{Int}`) share
    /// the `struct`-declared base, so both report `true`; an abstract parametric
    /// instantiation (`AbsP{Int}`) is excluded by the `!is_abstract_type`
    /// guard, matching upstream (Issue #5102).
    pub(crate) fn is_struct_type(&self) -> bool {
        // `Union{...}` / `Union{}` / a free `TypeVar` unwrap to a non-`DataType`
        // and are never struct types upstream.
        if matches!(
            self.handle.projection(),
            JuliaType::Union(_) | JuliaType::Bottom | JuliaType::TypeVar(..)
        ) {
            return false;
        }
        // `isstructtype` excludes primitive and abstract types.
        if self.is_abstract_type() || self.is_primitive_type() {
            return false;
        }
        let type_name = self.handle.projection().name();
        let base =
            parametric_base_name(type_name.as_ref()).unwrap_or_else(|| type_name.to_string());
        self.handle.core().is_builtin_struct_datatype()
            || CoreType::is_builtin_struct_datatype_for_julia_name(&base)
            || self.user_struct_def().is_some()
            || self.parametric_schema().is_some()
    }

    /// Mirror of upstream `isprimitivetype` (`base/runtime_internals.jl`):
    /// `unwrap_unionall(t)` must be a `DataType` declared with the
    /// `primitive type` syntax (`Int`, `Float64`, `Char`, ...). `CoreType` owns
    /// the builtin primitive set; user structs and `String` are not primitive.
    pub(crate) fn is_primitive_type(&self) -> bool {
        if matches!(
            self.handle.projection(),
            JuliaType::Union(_) | JuliaType::Bottom | JuliaType::TypeVar(..)
        ) {
            return false;
        }
        let type_name = self.handle.projection().name();
        let base =
            parametric_base_name(type_name.as_ref()).unwrap_or_else(|| type_name.to_string());
        // A user `primitive type Name Bits end` is a primitive type too (Issue #5058).
        if self.primitive_def().is_some() {
            return true;
        }
        CoreType::is_builtin_primitive_datatype_for_julia_name(&base)
    }

    /// Mirror of upstream `ismutabletype` (`base/runtime_internals.jl`):
    /// `unwrap_unionall(t)` must be a `DataType` whose `TypeName` carries the
    /// mutable flag. `Union`/`TypeVar`/`Bottom` are not mutable.
    ///
    /// Base-name resolution supplies the `unwrap_unionall` behaviour: a mutable
    /// user struct keeps its mutability across instantiations (`MutBox{Int}`
    /// stays mutable), and the bare builtin mutable containers (`Array`,
    /// `Vector`, `Dict`, ...) report `true` as upstream (Issue #5102).
    pub(crate) fn is_mutable_type(&self) -> bool {
        if matches!(
            self.handle.projection(),
            JuliaType::Union(_) | JuliaType::Bottom | JuliaType::TypeVar(..)
        ) {
            return false;
        }
        let type_name = self.handle.projection().name();
        let base =
            parametric_base_name(type_name.as_ref()).unwrap_or_else(|| type_name.to_string());
        self.handle.core().is_builtin_mutable_datatype()
            || CoreType::is_builtin_mutable_datatype_for_julia_name(&base)
            || self.user_struct_def().is_some_and(|def| def.is_mutable)
            // A *parametric* user struct (`mutable struct MBox{T}`) keeps its
            // declared mutability across instantiations; its schema lives in the
            // compile context keyed by base name, not in `struct_defs`.
            || self
                .parametric_schema()
                .is_some_and(|schema| schema.def.is_mutable)
    }

    fn parametric_type_params(&self, name: &str) -> Option<Vec<JuliaType>> {
        if let Some(ctx) = self.registry.compile_context {
            if let Some(def) = ctx.parametric_structs.get(name) {
                let params = def
                    .def
                    .type_params
                    .iter()
                    .map(|tp| JuliaType::TypeVar(tp.name.clone(), tp.upper_bound.clone()))
                    .collect();
                return Some(params);
            }
        }

        self.registry
            .abstract_types
            .iter()
            .find(|def| def.name == name && !def.type_params.is_empty())
            .map(|def| {
                def.type_params
                    .iter()
                    .map(|name| JuliaType::TypeVar(name.clone(), None))
                    .collect()
            })
    }

    fn user_struct_def(&self) -> Option<&StructDefInfo> {
        let type_name = self.handle.projection().name();
        let type_name = type_name.as_ref();
        // Exact match first (covers non-parametric structs and any
        // monomorphized `Box{Int64}` def that was registered directly).
        if let Some(def) = self
            .registry
            .struct_defs
            .iter()
            .find(|d| d.name == type_name)
        {
            return Some(def);
        }
        // For a parametric instantiation such as `Box{Int64}`, the registered
        // `StructDefInfo` is keyed by the base name `Box`. Resolving through the
        // base keeps `fieldnames`/`fieldcount` consistent with upstream Julia,
        // which reports the same field names regardless of the type arguments.
        let base = parametric_base_name(type_name)?;
        if base == type_name {
            return None;
        }
        self.registry
            .struct_defs
            .iter()
            .find(|def| def.name == base)
    }

    /// Resolve the parametric struct schema (keyed by base name) for this
    /// type's projection, covering both the bare `Box` and the instantiated
    /// `Box{Int64}` forms.
    fn parametric_schema(&self) -> Option<&'_ crate::compile::ParametricStructDef> {
        let type_name = self.handle.projection().name();
        let base = parametric_base_name(type_name.as_ref())?;
        self.registry.compile_context?.parametric_structs.get(&base)
    }

    /// Resolve a user-declared primitive type (`primitive type Name Bits end`,
    /// Issue #5058) for this type's projection. The registry is keyed by name,
    /// which lives on the (reconstructed) `RuntimeCompileContext`. Returns `None`
    /// for builtin types, structs, abstract types, and unions.
    fn primitive_def(&self) -> Option<&'_ crate::vm::types::PrimitiveTypeDefInfo> {
        if matches!(
            self.handle.projection(),
            JuliaType::Union(_) | JuliaType::Bottom | JuliaType::TypeVar(..) | JuliaType::TypeOf(_)
        ) {
            return None;
        }
        let type_name = self.handle.projection().name();
        let base =
            parametric_base_name(type_name.as_ref()).unwrap_or_else(|| type_name.to_string());
        self.registry
            .compile_context?
            .primitive_types
            .iter()
            .find(|p| p.name == base)
    }

    /// Field `JuliaType`s for a *parametric* user struct instantiation such as
    /// `Box{Int64}`, with the declared type parameters substituted by the
    /// concrete type arguments. Returns `None` when the projection is not a
    /// parametric instantiation backed by a known parametric schema.
    fn parametric_field_types(&self) -> Option<Vec<JuliaType>> {
        let type_name = self.handle.projection().name();
        let type_name = type_name.as_ref();
        let (base, args) = split_parametric_name(type_name)?;
        let ctx = self.registry.compile_context?;
        let def = ctx.parametric_structs.get(&base)?;
        if def.def.type_params.is_empty() {
            return None;
        }
        // Build the type-parameter -> concrete-argument substitution map. When
        // fewer arguments than parameters are supplied (a partially applied
        // form), only the leading parameters are substituted.
        let subst: std::collections::HashMap<&str, &JuliaType> = def
            .def
            .type_params
            .iter()
            .map(|tp| tp.name.as_str())
            .zip(args.iter())
            .collect();
        Some(
            def.def
                .fields
                .iter()
                .map(|field| match &field.type_expr {
                    Some(expr) => expr.substitute_to_julia_type_lossy(&subst),
                    None => JuliaType::Any,
                })
                .collect(),
        )
    }

    fn builtin_field_offsets(&self) -> Option<Vec<usize>> {
        let fields = self.handle.core().builtin_field_metadata()?;
        let mut offset = 0usize;
        let mut offsets = Vec::with_capacity(fields.len());
        for (_, field_type) in fields {
            let (size, align) = core_type_layout(&field_type);
            offset = align_to_runtime_type(offset, align);
            offsets.push(offset);
            offset = offset.checked_add(size)?;
        }
        Some(offsets)
    }
}

/// Whether a `JuliaType` projection carries a *free* type variable, used by
/// [`RuntimeTypeObject::is_concrete_type`] to mirror upstream Julia's
/// `dt->hasfreetypevars` check. A free type variable makes a `DataType`
/// non-concrete (e.g. `Box{T}`), whereas a closed `UnionAll`-valued parameter
/// (e.g. the `Vector` in `Box{Vector}`) does not.
///
/// Parametric struct projections embed their arguments inside the name string
/// (`"Box{Int64}"`), so the `{...}` payload is parsed and each argument is
/// recursively inspected rather than relying on nested enum structure.
fn projection_has_free_typevars(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::TypeVar(..) => true,
        JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
            projection_has_free_typevars(inner)
        }
        JuliaType::TupleOf(elements) | JuliaType::Union(elements) => {
            elements.iter().any(projection_has_free_typevars)
        }
        // A `UnionAll` is a closed binder; its bound variable is not free, and a
        // `UnionAll`-valued parameter (`Box{Vector}`) stays concrete upstream.
        JuliaType::UnionAll { .. } => false,
        JuliaType::Struct(name) => {
            // Parametric instantiation names (`"Box{Int64}"`) keep their type
            // arguments in the string; parse and recurse so a free type
            // variable argument (`"Box{T}"`) is detected.
            match split_parametric_name(name) {
                Some((_, args)) => args.iter().any(projection_has_free_typevars),
                // A bare name that is itself a type-variable spelling (`"T"`)
                // is a free type variable.
                None => is_type_variable_spelling(name),
            }
        }
        _ => false,
    }
}

/// Whether a bare type name is spelled like an anonymous/explicit type variable
/// (`T`, `S`, `T1`), used when a parametric argument was parsed into a
/// `JuliaType::Struct(name)` rather than a `JuliaType::TypeVar`.
fn is_type_variable_spelling(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => chars.all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

/// Whether a *bare* (no `{...}` arguments) type name is the upstream-abstract
/// `Ref` UnionAll. Upstream Julia declares `abstract type Ref{T} end`, so the
/// un-instantiated `Ref` is a `UnionAll` and `Ref{T}` instantiations are
/// abstract `DataType`s (`isabstracttype(Ref) == true`,
/// `isconcretetype(Ref{Int}) == false`). sjulia models `Ref`/`Base.RefValue`
/// as a single interior-mutable box (Issue #5130), so this layer reclassifies
/// the abstract-`Ref` half explicitly (Issue #5223).
fn is_ref_abstract_base_name(base: &str) -> bool {
    base == "Ref"
}

/// Whether a *bare* (no `{...}` arguments) type name is part of the `Ref`
/// family — either the abstract `Ref` UnionAll or the concrete
/// `Base.RefValue` struct UnionAll. Both are `UnionAll`s upstream
/// (`typeof(Ref) === typeof(Base.RefValue) === UnionAll`), so a bare reference
/// to either projects to a `UnionAll`-kind type object (Issue #5223).
fn is_bare_ref_family_name(name: &str) -> bool {
    matches!(name, "Ref" | "RefValue" | "Base.RefValue")
}

/// Resolve a (possibly parametric / `where`-wrapped / aliased) type display
/// name to its canonical `TypeName` symbol (Issue #5106).
///
/// Strips any ` where ...` suffix and `{...}` parameters, then collapses the
/// `Array` Base aliases (`Vector`, `Matrix`) onto `Array` so that the shared
/// upstream `TypeName` is reported. User and other builtin names pass through
/// unchanged.
/// Base name + raw top-level argument tokens of a (possibly `where`-suffixed)
/// type display name.
///
/// Issue #6336: the tokenizing is delegated to the central type-core name
/// parser (`inference_core::parse_parametric_type_name`, the same tokenizer
/// behind `CoreType::from_julia_name`) instead of local `find('{')` /
/// comma-splitting copies. The trailing `where` clause is dropped first
/// (`Vector{T} where T` -> `Vector{T}`), matching the previous helpers'
/// treatment of `where`-wrapped display names.
fn parse_display_type_name(type_name: &str) -> (&str, Vec<&str>) {
    let without_where = match type_name.find(" where ") {
        Some(idx) => &type_name[..idx],
        None => type_name,
    };
    let (base, args) = crate::inference_core::parse_parametric_type_name(without_where.trim());
    (base.trim(), args)
}

fn canonical_typename(type_name: &str) -> String {
    let (base, _) = parse_display_type_name(type_name);
    match base {
        // `Array{T,1}`/`Vector{T}` and `Array{T,2}`/`Matrix{T}` share the
        // `Array` TypeName upstream; the bare `Vector`/`Matrix` UnionAlls do
        // too. Strip the Base namespace prefix defensively.
        "Vector" | "Matrix" | "Base.Vector" | "Base.Matrix" => "Array".to_string(),
        "DenseVector" | "DenseMatrix" | "Base.DenseVector" | "Base.DenseMatrix" => {
            "DenseArray".to_string()
        }
        _ => base.to_string(),
    }
}

/// Extract the base name of a (possibly parametric) type name. For `Box{Int64}`
/// this returns `Box`; for a non-parametric name the name is returned unchanged.
fn parametric_base_name(type_name: &str) -> Option<String> {
    Some(parse_display_type_name(type_name).0.to_string())
}

/// Strip a parametric `{...}` suffix from a type name, yielding the bare base
/// name (`AbsB{T}` -> `AbsB`, `AbsB` -> `AbsB`). Used so `subtypes` can match a
/// declared parent against the queried type by base name, since a declared
/// parent may carry bound type vars (`<: AbsB{T}`).
fn base_name_without_params(name: &str) -> &str {
    parse_display_type_name(name).0
}

/// Strip a module qualifier prefix from a (already `{...}`-stripped) type name,
/// yielding the module-unqualified tail (`M2.E` -> `E`, `E` -> `E`). Module
/// nesting separators are `.`, so the segment after the final `.` is the name a
/// reference inside the declaring module sees.
fn module_unqualified_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// Whether a declared type's (possibly module-qualified, possibly parametric)
/// `def_name` names the queried base `query`. A bare query (`E`) matches both a
/// bare declaration (`E`) and a module-qualified one (`M2.E`) by its
/// module-unqualified tail, so a module-private short type name referenced as a
/// value resolves to its real `DataType`/`UnionAll` rather than being mistaken
/// for a free type variable by the single-letter type-variable heuristic
/// (Issue #8100). A qualified query is matched only exactly, so it cannot
/// loosely match a same-named type in a different module.
fn declared_name_matches_base(def_name: &str, query: &str) -> bool {
    let def_base = base_name_without_params(def_name);
    def_base == query || (!query.contains('.') && module_unqualified_name(def_base) == query)
}

/// Whether a declared parent (possibly parametric, possibly absent — defaulting
/// to `Any`) names the given parent base type. Compares by base name so
/// `<: AbsB{T}` matches a query for `AbsB`.
fn parent_base_matches(declared_parent: Option<&str>, parent_base: &str) -> bool {
    base_name_without_params(declared_parent.unwrap_or("Any")) == parent_base
}

/// Split a parametric type name `Base{A, B, ...}` into its base name and the
/// parsed concrete type arguments. Returns `None` when the name is not a
/// parametric instantiation (no `{...}` suffix). Tokenizing goes through the
/// central type-core name parser (Issue #6336).
fn split_parametric_name(type_name: &str) -> Option<(String, Vec<JuliaType>)> {
    let (base, args) = parse_display_type_name(type_name);
    if args.is_empty() {
        return None;
    }
    let args = args
        .into_iter()
        .filter(|arg| !arg.is_empty())
        .map(JuliaType::from_name_or_struct)
        .collect();
    Some((base.to_string(), args))
}

fn dense_array_alias_supertype_name(type_name: &str) -> Option<String> {
    let (base, args) = split_parametric_name(type_name)?;
    let elem = args.first()?.name();
    Some(match base.as_str() {
        "Vector" => format!("DenseVector{{{elem}}}"),
        "Matrix" => format!("DenseMatrix{{{elem}}}"),
        "DenseVector" => format!("AbstractVector{{{elem}}}"),
        "DenseMatrix" => format!("AbstractMatrix{{{elem}}}"),
        "Array" if args.len() >= 2 => match args[1].name().as_ref() {
            "1" => format!("DenseVector{{{elem}}}"),
            "2" => format!("DenseMatrix{{{elem}}}"),
            rank => format!("DenseArray{{{elem}, {rank}}}"),
        },
        "DenseArray" if args.len() >= 2 => match args[1].name().as_ref() {
            "1" => format!("AbstractVector{{{elem}}}"),
            "2" => format!("AbstractMatrix{{{elem}}}"),
            rank => format!("AbstractArray{{{elem}, {rank}}}"),
        },
        _ => return None,
    })
}

/// Compute the supertype *name* of a user type given its declared parent and the
/// concrete arguments applied to the type. When the declared parent is itself
/// parametric (e.g. `AbsB{T}`), the bound type variables are substituted with
/// the concrete arguments. A missing parent resolves to `Any` (fixing the prior
/// behavior where a parametric instantiation reported itself as its supertype).
fn substitute_parent_name(
    parent: Option<&str>,
    type_params: &[String],
    args: &[JuliaType],
) -> String {
    let parent = match parent {
        Some(p) => p.trim(),
        None => return "Any".to_string(),
    };
    let parent_base = match parent.find('{') {
        Some(idx) => parent[..idx].trim().to_string(),
        None => return parent.to_string(),
    };
    let parent_args = parametric_arg_tokens(parent);
    let subst: std::collections::HashMap<&str, &JuliaType> = type_params
        .iter()
        .map(String::as_str)
        .zip(args.iter())
        .collect();
    let rendered: Vec<String> = parent_args
        .iter()
        .map(|tok| substitute_type_vars_in_token(tok, &subst))
        .collect();
    format!("{}{{{}}}", parent_base, rendered.join(", "))
}

/// Substitute bound type variables inside a single parent argument token.
///
/// A parent declaration can thread its type variables through a *nested*
/// parametric expression, e.g. `StaticVecOrMat{Tuple{N}, T, 1}`. Plain
/// top-level token matching only rewrites the bare `T` arg and leaves `Tuple{N}`
/// untouched, so the value parameter `N` never reaches the grandparent
/// (`AbstractArray{T,N}`) and the subtype edge is lost (Issue #7728 / #7819).
/// Recurse into any `Base{...}` token so type vars nested at any depth are
/// substituted before the parent name is re-rendered.
fn substitute_type_vars_in_token(
    token: &str,
    subst: &std::collections::HashMap<&str, &JuliaType>,
) -> String {
    let trimmed = token.trim();
    if let Some(t) = subst.get(trimmed) {
        return t.name().to_string();
    }
    let (base, inner_args) = parse_display_type_name(trimmed);
    if inner_args.is_empty() {
        return trimmed.to_string();
    }
    let rendered: Vec<String> = inner_args
        .iter()
        .map(|inner| substitute_type_vars_in_token(inner, subst))
        .collect();
    format!("{}{{{}}}", base, rendered.join(", "))
}

/// Tokenize the argument list of a parametric parent name into raw strings,
/// preserving type-variable identifiers so they can be substituted by name.
/// Tokenizing goes through the central type-core name parser (Issue #6336).
fn parametric_arg_tokens(parent: &str) -> Vec<String> {
    parse_display_type_name(parent)
        .1
        .into_iter()
        .filter(|tok| !tok.is_empty())
        .map(str::to_string)
        .collect()
}

fn align_to_runtime_type(offset: usize, align: usize) -> usize {
    if align <= 1 {
        offset
    } else {
        offset.div_ceil(align) * align
    }
}

fn core_type_layout(field_type: &CoreType) -> (usize, usize) {
    match field_type {
        CoreType::Primitive(CorePrimitive::Bool)
        | CoreType::Primitive(CorePrimitive::Int8)
        | CoreType::Primitive(CorePrimitive::UInt8) => (1, 1),
        CoreType::Primitive(CorePrimitive::Int16)
        | CoreType::Primitive(CorePrimitive::UInt16)
        | CoreType::Primitive(CorePrimitive::Float16) => (2, 2),
        CoreType::Primitive(CorePrimitive::Int32)
        | CoreType::Primitive(CorePrimitive::UInt32)
        | CoreType::Primitive(CorePrimitive::Float32)
        | CoreType::Primitive(CorePrimitive::Char) => (4, 4),
        CoreType::Primitive(CorePrimitive::Int64)
        | CoreType::Primitive(CorePrimitive::UInt64)
        | CoreType::Primitive(CorePrimitive::Float64) => (8, 8),
        CoreType::Primitive(CorePrimitive::Int128)
        | CoreType::Primitive(CorePrimitive::UInt128) => (16, 16),
        CoreType::Primitive(CorePrimitive::Nothing)
        | CoreType::Primitive(CorePrimitive::Missing) => (0, 1),
        _ => (8, 8),
    }
}

/// A single `.parameters` entry as surfaced by reflection. Type parameters map
/// to a `JuliaType`; integer/value parameters (e.g. array dimensionality `N`,
/// `Val{5}`, `Val{:foo}`) are carried as concrete value kinds so the
/// reflection consumers can build `svec(Int64, 1)`-style results that match
/// upstream Julia exactly (Issue #5162).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReflectionParameter {
    Type(JuliaType),
    Int(i64),
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int128(i128),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    UInt128(u128),
    Bool(bool),
    Symbol(String),
    Str(String),
}

/// Map a single core type parameter to a reflection parameter, preserving
/// integer/value parameters that `core_type_to_reflection_julia_type` drops.
fn reflection_parameter_from_core(core: &CoreType) -> Option<ReflectionParameter> {
    if let CoreType::Value(value) = core {
        return Some(match value {
            CoreValueParam::Int(n) => ReflectionParameter::Int(*n),
            CoreValueParam::SignedInt { bits, value } => match bits {
                8 => ReflectionParameter::Int8(*value as i8),
                16 => ReflectionParameter::Int16(*value as i16),
                32 => ReflectionParameter::Int32(*value as i32),
                128 => ReflectionParameter::Int128(*value),
                _ => return None,
            },
            CoreValueParam::UnsignedInt { bits, value } => match bits {
                8 => ReflectionParameter::UInt8(*value as u8),
                16 => ReflectionParameter::UInt16(*value as u16),
                32 => ReflectionParameter::UInt32(*value as u32),
                64 => ReflectionParameter::UInt64(*value as u64),
                128 => ReflectionParameter::UInt128(*value),
                _ => return None,
            },
            CoreValueParam::Bool(b) => ReflectionParameter::Bool(*b),
            CoreValueParam::Symbol(s) => ReflectionParameter::Symbol(s.clone()),
            CoreValueParam::String(s) => ReflectionParameter::Str(s.clone()),
        });
    }
    core_type_to_reflection_julia_type(core).map(ReflectionParameter::Type)
}

pub(crate) fn core_type_to_reflection_julia_type(core: &CoreType) -> Option<JuliaType> {
    if let CoreType::TypeVar(var) = core {
        return Some(JuliaType::TypeVar(
            var.name.clone(),
            var.upper_bound.as_ref().map(|bound| bound.to_julia_name()),
        ));
    }

    let name = core.to_julia_name();
    if name.chars().all(|c| c.is_ascii_digit()) {
        None
    } else {
        Some(JuliaType::from_name_or_struct(&name))
    }
}

fn nested_unionall_body(base_name: &str, params: Vec<JuliaType>) -> JuliaType {
    let param_names: Vec<String> = params
        .iter()
        .map(|param| param.name().to_string())
        .collect();
    let mut body = JuliaType::Struct(format!("{}{{{}}}", base_name, param_names.join(", ")));

    for param in params.into_iter().skip(1).rev() {
        if let JuliaType::TypeVar(var, bound) = param {
            body = JuliaType::UnionAll {
                var,
                lower_bound: None,
                bound: bound.map(Box::new),
                body: Box::new(body),
            };
        }
    }

    body
}

/// Thin delegating wrapper over the canonical VM-side `ValueType → JuliaType`
/// conversion in `vm/builtins_reflection/primitives.rs` (Issue #5916).
///
/// The previous local copy diverged on `Union`: it collapsed every
/// `ValueType::Union(..)` to `Any`, while the canonical conversion preserves
/// the union structurally (`Union{...}`, empty union → `Union{}` /
/// `JuliaType::Bottom`), matching upstream `fieldtype` / `fieldtypes` output
/// for `Union`-typed fields.
fn value_type_to_reflection_julia_type(vt: &ValueType, struct_defs: &[StructDefInfo]) -> JuliaType {
    super::builtins_reflection::primitives::value_type_to_julia_type(vt, struct_defs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::ValueType;

    /// Issue #5916: the reflection `ValueType → JuliaType` path now delegates
    /// to the canonical conversion, which preserves unions structurally
    /// (the old local copy collapsed every `Union` to `Any`).
    #[test]
    fn reflection_value_type_union_is_preserved_issue_5916() {
        let union = ValueType::Union(vec![ValueType::Nothing, ValueType::I64]);
        assert_eq!(
            value_type_to_reflection_julia_type(&union, &[]),
            JuliaType::Union(vec![JuliaType::Nothing, JuliaType::Int64])
        );
        // Empty union is `Union{}` (Bottom), not `Any`.
        assert_eq!(
            value_type_to_reflection_julia_type(&ValueType::Union(Vec::new()), &[]),
            JuliaType::Bottom
        );
    }

    #[test]
    fn runtime_type_object_preserves_nested_parameters() {
        let registry = RuntimeTypeRegistry::new(None, &[]);
        let ty = JuliaType::Struct("Dict{String, Vector{Int64}}".to_string());
        let params = registry.object(&ty).parameters_with_values();

        assert_eq!(params.len(), 2);
        assert_eq!(params[0], ReflectionParameter::Type(JuliaType::String));
        assert_eq!(
            params[1],
            ReflectionParameter::Type(JuliaType::VectorOf(Box::new(JuliaType::Int64)))
        );
    }

    #[test]
    fn runtime_type_object_emits_value_parameters_issue_5162() {
        let registry = RuntimeTypeRegistry::new(None, &[]);

        // Vector{T} alias expands to Array{T, 1}.
        let vector = JuliaType::VectorOf(Box::new(JuliaType::Int64));
        assert_eq!(
            registry.object(&vector).parameters_with_values(),
            vec![
                ReflectionParameter::Type(JuliaType::Int64),
                ReflectionParameter::Int(1),
            ]
        );

        // Matrix{T} alias expands to Array{T, 2}.
        let matrix = JuliaType::MatrixOf(Box::new(JuliaType::Float64));
        assert_eq!(
            registry.object(&matrix).parameters_with_values(),
            vec![
                ReflectionParameter::Type(JuliaType::Float64),
                ReflectionParameter::Int(2),
            ]
        );

        // Array{T, N} already carries its dimension value parameter.
        let array = JuliaType::Struct("Array{Int64, 3}".to_string());
        assert_eq!(
            registry.object(&array).parameters_with_values(),
            vec![
                ReflectionParameter::Type(JuliaType::Int64),
                ReflectionParameter::Int(3),
            ]
        );

        // Val carries a pure value parameter.
        let val_int = JuliaType::Struct("Val{5}".to_string());
        assert_eq!(
            registry.object(&val_int).parameters_with_values(),
            vec![ReflectionParameter::Int(5)]
        );
        let val_u8 = JuliaType::Struct("Val{0x01}".to_string());
        assert_eq!(
            registry.object(&val_u8).parameters_with_values(),
            vec![ReflectionParameter::UInt8(1)]
        );
        let val_u8_static = JuliaType::Struct("Val{UInt8(1)}".to_string());
        assert_eq!(
            registry.object(&val_u8_static).parameters_with_values(),
            vec![ReflectionParameter::UInt8(1)]
        );
        let val_i32 = JuliaType::Struct("Val{Int32(2)}".to_string());
        assert_eq!(
            registry.object(&val_i32).parameters_with_values(),
            vec![ReflectionParameter::Int32(2)]
        );
        let val_sym = JuliaType::Struct("Val{:foo}".to_string());
        assert_eq!(
            registry.object(&val_sym).parameters_with_values(),
            vec![ReflectionParameter::Symbol("foo".to_string())]
        );
        let val_bool = JuliaType::Struct("Val{true}".to_string());
        assert_eq!(
            registry.object(&val_bool).parameters_with_values(),
            vec![ReflectionParameter::Bool(true)]
        );
    }

    #[test]
    fn runtime_type_object_exposes_unionall_var_and_body() {
        let registry = RuntimeTypeRegistry::new(None, &[]);
        let ty = JuliaType::Dict;
        let object = registry.object(&ty);

        assert_eq!(object.kind(), RuntimeTypeObjectKind::UnionAll);
        assert_eq!(
            object.unionall_var(),
            Some(JuliaType::TypeVar("K".to_string(), None))
        );
        assert_eq!(
            object.unionall_body(),
            Some(JuliaType::UnionAll {
                lower_bound: None,
                var: "V".to_string(),
                bound: None,
                body: Box::new(JuliaType::Struct("Dict{K, V}".to_string()))
            })
        );
    }

    #[test]
    fn runtime_type_object_exposes_typevar_metadata() {
        let registry = RuntimeTypeRegistry::new(None, &[]);
        let bounded = JuliaType::TypeVar("T".to_string(), Some("Integer".to_string()));
        let object = registry.object(&bounded);

        assert_eq!(object.kind(), RuntimeTypeObjectKind::TypeVar);
        assert_eq!(object.typevar_name(), Some("T".to_string()));
        assert_eq!(object.typevar_lower_bound(), Some(JuliaType::Bottom));
        assert_eq!(object.typevar_upper_bound(), Some(JuliaType::Integer));

        let unbounded = JuliaType::TypeVar("S".to_string(), None);
        assert_eq!(
            registry.object(&unbounded).typevar_upper_bound(),
            Some(JuliaType::Any)
        );
    }

    #[test]
    fn runtime_registry_preserves_ast_builtin_datatype_flags() {
        let registry = RuntimeTypeRegistry::new(None, &[]);

        for ty in [
            JuliaType::Expr,
            JuliaType::QuoteNode,
            JuliaType::LineNumberNode,
            JuliaType::GlobalRef,
        ] {
            let object = registry.object(&ty);
            assert!(object.is_struct_type(), "{ty}");
            assert!(object.is_concrete_type(), "{ty}");
        }

        assert!(registry.object(&JuliaType::Expr).is_mutable_type());
        for ty in [
            JuliaType::QuoteNode,
            JuliaType::LineNumberNode,
            JuliaType::GlobalRef,
        ] {
            assert!(!registry.object(&ty).is_mutable_type(), "{ty}");
        }
    }

    #[test]
    fn runtime_registry_unifies_builtin_and_user_hierarchy_queries() {
        let abstract_types = vec![AbstractTypeDefInfo {
            name: "Animal".to_string(),
            parent: Some("Any".to_string()),
            type_params: vec![],
        }];
        let struct_defs = vec![StructDefInfo {
            name: "Dog".to_string(),
            is_mutable: true,
            fields: vec![("age".to_string(), ValueType::I64)],
            field_julia_types: vec![JuliaType::Int64],
            parent_type: Some("Animal".to_string()),
        }];
        let registry =
            RuntimeTypeRegistry::new_with_struct_defs(None, &abstract_types, &struct_defs);

        assert_eq!(registry.supertype_name("Int64"), "Signed");
        assert_eq!(registry.supertype_name("Dog"), "Animal");
        assert!(registry
            .direct_subtypes("Animal")
            .contains(&JuliaType::Struct("Dog".to_string())));

        let int = JuliaType::Int64;
        assert!(registry.object(&int).is_concrete_type());
        assert!(!registry.object(&int).is_abstract_type());

        let animal = JuliaType::Struct("Animal".to_string());
        assert!(registry.object(&animal).is_abstract_type());
        assert!(!registry.object(&animal).is_concrete_type());

        let dog = JuliaType::Struct("Dog".to_string());
        assert!(registry.object(&dog).is_struct_type());
        assert!(registry.object(&dog).is_concrete_type());
        assert!(registry.object(&dog).is_mutable_type());
    }

    #[test]
    fn direct_subtypes_dedups_builtin_and_registry_overlap() {
        // `Signed`/`Unsigned` are Base-defined abstract types that live in both
        // the builtin lattice AND the runtime `abstract_types` registry. Without
        // deduplication they appear twice. The result must also be sorted by
        // string name to match upstream `sortperm(map(string, sts))`.
        let abstract_types = vec![
            AbstractTypeDefInfo {
                name: "Signed".to_string(),
                parent: Some("Integer".to_string()),
                type_params: vec![],
            },
            AbstractTypeDefInfo {
                name: "Unsigned".to_string(),
                parent: Some("Integer".to_string()),
                type_params: vec![],
            },
        ];
        let registry = RuntimeTypeRegistry::new(None, &abstract_types);
        let names: Vec<String> = registry
            .direct_subtypes("Integer")
            .iter()
            .map(|t| t.name().to_string())
            .collect();
        // Sorted, deduplicated: matches `subtypes(Integer)`.
        assert_eq!(names, vec!["Bool", "Signed", "Unsigned"]);
    }

    #[test]
    fn direct_subtypes_lists_parametric_struct_base_not_instantiations() {
        // A user `struct Box{T} <: AbsB` should surface as the base `UnionAll`
        // name `Box`, never as the monomorphized `Box{Int64}` / `Box{Float64}`
        // that the compiler records in `struct_defs`. A declared parent carrying
        // bound type vars (`<: AbsB`) is matched by base name.
        let abstract_types = vec![AbstractTypeDefInfo {
            name: "AbsB".to_string(),
            parent: Some("Any".to_string()),
            type_params: vec![],
        }];
        // Monomorphized instantiations carry a `{...}` suffix and must be
        // filtered out; the non-parametric `Plain` is kept.
        let struct_defs = vec![
            StructDefInfo {
                name: "Box{Int64}".to_string(),
                is_mutable: false,
                fields: vec![("x".to_string(), ValueType::I64)],
                field_julia_types: vec![JuliaType::Int64],
                parent_type: Some("AbsB".to_string()),
            },
            StructDefInfo {
                name: "Plain".to_string(),
                is_mutable: false,
                fields: vec![],
                field_julia_types: vec![],
                parent_type: Some("AbsB".to_string()),
            },
        ];
        let registry =
            RuntimeTypeRegistry::new_with_struct_defs(None, &abstract_types, &struct_defs);
        let names: Vec<String> = registry
            .direct_subtypes("AbsB")
            .iter()
            .map(|t| t.name().to_string())
            .collect();
        // Without a parametric schema in the (None) compile context, only the
        // non-parametric `Plain` is recoverable; the `Box{Int64}` instantiation
        // is correctly excluded rather than leaking as a subtype.
        assert_eq!(names, vec!["Plain"]);
    }

    #[test]
    fn base_name_without_params_strips_suffix() {
        assert_eq!(base_name_without_params("AbsB{T}"), "AbsB");
        assert_eq!(base_name_without_params("AbsB"), "AbsB");
        assert_eq!(base_name_without_params("Pair{Int64, String}"), "Pair");
        assert!(parent_base_matches(Some("AbsB{T}"), "AbsB"));
        assert!(parent_base_matches(None, "Any"));
        assert!(!parent_base_matches(Some("AbsC"), "AbsB"));
    }

    #[test]
    fn split_parametric_name_extracts_base_and_args() {
        let (base, args) = split_parametric_name("Box{Int64}").unwrap();
        assert_eq!(base, "Box");
        assert_eq!(args, vec![JuliaType::Int64]);

        let (base, args) = split_parametric_name("Pair{Int64, String}").unwrap();
        assert_eq!(base, "Pair");
        assert_eq!(args, vec![JuliaType::Int64, JuliaType::String]);

        // Nested braces are respected when splitting arguments.
        let (base, args) = split_parametric_name("Wrap{Vector{Int64}, Float64}").unwrap();
        assert_eq!(base, "Wrap");
        assert_eq!(
            args,
            vec![
                JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                JuliaType::Float64
            ]
        );

        assert!(split_parametric_name("Int64").is_none());
    }

    #[test]
    fn concrete_tuple_field_types_are_elements_issue_8385() {
        let registry = RuntimeTypeRegistry::new(None, &[]);
        assert_eq!(
            registry
                .object(&JuliaType::TupleOf(vec![
                    JuliaType::Float64,
                    JuliaType::Float64
                ]))
                .field_types(),
            Some(vec![JuliaType::Float64, JuliaType::Float64])
        );
    }

    #[test]
    fn canonical_typename_collapses_array_aliases_issue_5106() {
        // Plain names pass through.
        assert_eq!(canonical_typename("Int64"), "Int64");
        assert_eq!(canonical_typename("Number"), "Number");
        assert_eq!(canonical_typename("Dict"), "Dict");

        // Type parameters are stripped to the base name.
        assert_eq!(canonical_typename("Dict{Int64, Int64}"), "Dict");
        assert_eq!(canonical_typename("Set{Int64}"), "Set");
        assert_eq!(canonical_typename("UnitRange{Int64}"), "UnitRange");
        assert_eq!(canonical_typename("Box{Int64}"), "Box");

        // `where` clauses are dropped before stripping parameters.
        assert_eq!(canonical_typename("Box{T} where T"), "Box");

        // The Array display aliases collapse onto the shared `Array` TypeName,
        // matching upstream `nameof(Vector{Int}) === :Array` (Issue #5106).
        assert_eq!(canonical_typename("Array"), "Array");
        assert_eq!(canonical_typename("Vector"), "Array");
        assert_eq!(canonical_typename("Vector{Int64}"), "Array");
        assert_eq!(canonical_typename("Matrix"), "Array");
        assert_eq!(canonical_typename("Matrix{Int64}"), "Array");
    }

    #[test]
    fn is_concrete_type_matches_upstream_boundary() {
        // Concrete parametric struct instantiations, function singleton types,
        // and the empty tuple type are concrete; UnionAll/bare-parametric types,
        // unions, the any-tuple `Tuple`, and tuples with abstract/UnionAll/
        // unbounded-Vararg element types are not (Issue #5203).
        let struct_defs = vec![StructDefInfo {
            name: "Box".to_string(),
            is_mutable: false,
            fields: vec![("x".to_string(), ValueType::I64)],
            field_julia_types: vec![JuliaType::Int64],
            parent_type: None,
        }];
        let registry = RuntimeTypeRegistry::new_with_struct_defs(None, &[], &struct_defs);
        let concrete = |name: &str| {
            registry
                .object(&JuliaType::Struct(name.to_string()))
                .is_concrete_type()
        };

        // Concrete parametric instantiation and function singleton.
        assert!(concrete("Box{Int64}"));
        assert!(concrete("Pair{Int64, String}"));
        assert!(concrete("typeof(f)"));
        // Abstract/UnionAll-valued parameters keep concreteness (no free vars).
        assert!(concrete("Box{Number}"));
        assert!(concrete("Box{Vector}"));

        // Empty tuple is concrete; tuples of concrete elements are concrete.
        assert!(registry
            .object(&JuliaType::TupleOf(vec![]))
            .is_concrete_type());
        assert!(registry
            .object(&JuliaType::TupleOf(vec![
                JuliaType::Int64,
                JuliaType::String
            ]))
            .is_concrete_type());

        // Free type variable in a parameter -> not concrete.
        assert!(!concrete("Box{T}"));

        // Bare any-tuple and union are not concrete.
        assert!(!registry.object(&JuliaType::Tuple).is_concrete_type());
        assert!(!registry
            .object(&JuliaType::Union(vec![JuliaType::Int64, JuliaType::String]))
            .is_concrete_type());

        // `Type{T}` kinds are not concrete.
        assert!(!registry
            .object(&JuliaType::TypeOf(Box::new(JuliaType::Int64)))
            .is_concrete_type());

        // Tuple with an abstract element type -> not concrete.
        assert!(!registry
            .object(&JuliaType::TupleOf(vec![JuliaType::Number]))
            .is_concrete_type());
        // Tuple with a UnionAll element type (bare `Vector`) -> not concrete.
        assert!(!registry
            .object(&JuliaType::TupleOf(vec![
                JuliaType::Int64,
                JuliaType::Array
            ]))
            .is_concrete_type());
    }

    #[test]
    fn substitute_type_expr_resolves_type_vars() {
        use crate::types::TypeExpr;
        let int = JuliaType::Int64;
        let string = JuliaType::String;
        let subst: std::collections::HashMap<&str, &JuliaType> =
            [("T", &int), ("S", &string)].into_iter().collect();

        assert_eq!(
            TypeExpr::TypeVar("T".to_string()).substitute_to_julia_type_lossy(&subst),
            JuliaType::Int64
        );
        // Unknown type vars widen to Any.
        assert_eq!(
            TypeExpr::TypeVar("U".to_string()).substitute_to_julia_type_lossy(&subst),
            JuliaType::Any
        );
        // Concrete field types pass through unchanged.
        assert_eq!(
            TypeExpr::Concrete(JuliaType::Bool).substitute_to_julia_type_lossy(&subst),
            JuliaType::Bool
        );
        // Parameterized field types are reconstructed with substituted args.
        let nested = TypeExpr::Parameterized {
            base: "Vector".to_string(),
            params: vec![TypeExpr::TypeVar("T".to_string())],
        };
        assert_eq!(
            nested.substitute_to_julia_type_lossy(&subst),
            JuliaType::VectorOf(Box::new(JuliaType::Int64))
        );
    }

    #[test]
    fn substitute_parent_name_handles_missing_and_parametric_parents() {
        // No declared parent => Any (never the type itself).
        assert_eq!(substitute_parent_name(None, &[], &[]), "Any");
        // Non-parametric parent passes through.
        assert_eq!(substitute_parent_name(Some("Animal"), &[], &[]), "Animal");
        // Parametric parent substitutes bound type vars with concrete args.
        let params = vec!["T".to_string()];
        let args = vec![JuliaType::Int64];
        assert_eq!(
            substitute_parent_name(Some("AbsB{T}"), &params, &args),
            "AbsB{Int64}"
        );
        // Multiple params, only the bound one is substituted.
        let params = vec!["A".to_string(), "B".to_string()];
        let args = vec![JuliaType::Int64, JuliaType::Float64];
        assert_eq!(
            substitute_parent_name(Some("AbsB{A}"), &params, &args),
            "AbsB{Int64}"
        );
        // A type variable NESTED inside a parent argument (`Tuple{N}`) must be
        // substituted too, so the value parameter reaches the grandparent
        // (Issue #7728 / #7819). Plain top-level token matching left `Tuple{N}`
        // verbatim and the `N` never propagated.
        let params = vec!["N".to_string(), "T".to_string()];
        let args = vec![JuliaType::from_name_or_struct("3"), JuliaType::Int64];
        assert_eq!(
            substitute_parent_name(Some("StaticVecOrMat{Tuple{N},T,1}"), &params, &args),
            "StaticVecOrMat{Tuple{3}, Int64, 1}"
        );
    }

    #[test]
    fn supertype_name_does_not_self_reference_for_parametric_instantiation() {
        // A user struct with no compile context / parametric schema must report
        // `Any` for a parametric instantiation, not echo the base name back.
        let struct_defs = vec![StructDefInfo {
            name: "Box".to_string(),
            is_mutable: false,
            fields: vec![("x".to_string(), ValueType::I64)],
            field_julia_types: vec![JuliaType::Int64],
            parent_type: None,
        }];
        let registry = RuntimeTypeRegistry::new_with_struct_defs(None, &[], &struct_defs);
        assert_eq!(registry.supertype_name("Box{Int64}"), "Any");
        assert_eq!(registry.supertype_name("Box"), "Any");
    }

    #[test]
    fn supertype_name_preserves_dense_array_alias_params_issue_3909() {
        let registry = RuntimeTypeRegistry::new(None, &[]);
        assert_eq!(
            registry.supertype_name("Vector{Int64}"),
            "DenseVector{Int64}"
        );
        assert_eq!(
            registry.supertype_name("Matrix{Float64}"),
            "DenseMatrix{Float64}"
        );
        assert_eq!(
            registry.supertype_name("DenseVector{Int64}"),
            "AbstractVector{Int64}"
        );
        assert_eq!(
            registry.supertype_name("DenseMatrix{Float64}"),
            "AbstractMatrix{Float64}"
        );
        assert_eq!(
            registry.supertype_name("Array{Int64, 1}"),
            "DenseVector{Int64}"
        );
        assert_eq!(
            registry.supertype_name("Array{Int64, 2}"),
            "DenseMatrix{Int64}"
        );
        assert_eq!(
            registry.supertype_name("Array{Int64, 3}"),
            "DenseArray{Int64, 3}"
        );
        assert_eq!(
            registry.supertype_name("DenseArray{Int64, 1}"),
            "AbstractVector{Int64}"
        );
        assert_eq!(
            registry.supertype_name("DenseArray{Int64, 2}"),
            "AbstractMatrix{Int64}"
        );
        assert_eq!(
            registry.supertype_name("DenseArray{Int64, 3}"),
            "AbstractArray{Int64, 3}"
        );
    }

    #[test]
    fn runtime_type_identity_separates_kind_from_semantics() {
        let registry = RuntimeTypeRegistry::new(None, &[]);

        let datatype = registry.object(&JuliaType::Int64);
        let unionall = registry.object(&JuliaType::Array);
        let typevar = registry.object(&JuliaType::TypeVar("T".to_string(), None));

        assert_eq!(datatype.kind(), RuntimeTypeObjectKind::DataType);
        assert_eq!(unionall.kind(), RuntimeTypeObjectKind::UnionAll);
        assert_eq!(typevar.kind(), RuntimeTypeObjectKind::TypeVar);

        assert_ne!(
            datatype.identity().stable_hash(),
            unionall.identity().stable_hash()
        );
        assert_eq!(
            registry.object(&JuliaType::Int64).identity().stable_hash(),
            datatype.identity().stable_hash()
        );
    }

    #[test]
    fn runtime_registry_classifies_bare_ref_as_abstract_unionall_issue_5223() {
        let registry = RuntimeTypeRegistry::new(None, &[]);

        // Upstream `Ref` is `abstract type Ref{T} end`: the bare form is a
        // `UnionAll` and `Ref{T}` instantiations are abstract `DataType`s.
        let bare_ref = registry.object(&JuliaType::Struct("Ref".to_string()));
        assert_eq!(bare_ref.kind(), RuntimeTypeObjectKind::UnionAll);
        assert!(bare_ref.is_abstract_type());
        assert!(!bare_ref.is_concrete_type());
        assert!(!bare_ref.is_struct_type());

        let ref_int = registry.object(&JuliaType::Struct("Ref{Int64}".to_string()));
        assert_eq!(ref_int.kind(), RuntimeTypeObjectKind::DataType);
        assert!(ref_int.is_abstract_type());
        assert!(!ref_int.is_concrete_type());
        assert!(!ref_int.is_struct_type());
    }

    #[test]
    fn runtime_registry_classifies_ref_value_as_concrete_struct_issue_5223() {
        let registry = RuntimeTypeRegistry::new(None, &[]);

        // `Base.RefValue` is `mutable struct RefValue{T} <: Ref{T}`: the bare
        // form is a (non-abstract) `UnionAll` struct family and
        // `RefValue{T}` instantiations are concrete structs.
        for bare in ["RefValue", "Base.RefValue"] {
            let object = registry.object(&JuliaType::Struct(bare.to_string()));
            assert_eq!(object.kind(), RuntimeTypeObjectKind::UnionAll, "{bare}");
            assert!(!object.is_abstract_type(), "{bare}");
            assert!(!object.is_concrete_type(), "{bare}");
            assert!(object.is_struct_type(), "{bare}");
        }

        let ref_value_int = registry.object(&JuliaType::Struct("Base.RefValue{Int64}".to_string()));
        assert_eq!(ref_value_int.kind(), RuntimeTypeObjectKind::DataType);
        assert!(!ref_value_int.is_abstract_type());
        assert!(ref_value_int.is_concrete_type());
        assert!(ref_value_int.is_struct_type());
    }

    #[test]
    fn single_letter_declared_struct_classifies_as_datatype_issue_5252() {
        // A single uppercase-letter name collides with the type-variable
        // spelling, so `CoreType::from(Struct("P"))` is a `TypeVar`. When `P` is
        // a *declared* struct it must still classify as a concrete `DataType`,
        // while an *undeclared* single letter (`Z`) stays a `TypeVar`.
        let struct_defs = vec![StructDefInfo {
            name: "P".to_string(),
            is_mutable: false,
            fields: vec![
                ("x".to_string(), ValueType::I64),
                ("y".to_string(), ValueType::I64),
            ],
            field_julia_types: vec![JuliaType::Int64, JuliaType::Int64],
            parent_type: None,
        }];
        let abstract_types = vec![AbstractTypeDefInfo {
            name: "N".to_string(),
            parent: None,
            type_params: vec![],
        }];
        let registry =
            RuntimeTypeRegistry::new_with_struct_defs(None, &abstract_types, &struct_defs);

        // Declared single-letter struct -> concrete DataType.
        let p = registry.object(&JuliaType::Struct("P".to_string()));
        assert_eq!(p.kind(), RuntimeTypeObjectKind::DataType);
        assert!(p.is_concrete_type());
        assert!(!p.is_abstract_type());
        assert!(p.is_struct_type());

        // Declared single-letter abstract type -> abstract DataType, not TypeVar.
        let n = registry.object(&JuliaType::Struct("N".to_string()));
        assert_eq!(n.kind(), RuntimeTypeObjectKind::DataType);
        assert!(n.is_abstract_type());
        assert!(!n.is_concrete_type());

        // Undeclared single letter -> genuine TypeVar (legitimate `where Z`).
        let z = registry.object(&JuliaType::Struct("Z".to_string()));
        assert_eq!(z.kind(), RuntimeTypeObjectKind::TypeVar);
        assert!(!z.is_concrete_type());

        // A parametric instantiation with a genuinely free type-variable
        // argument is still non-concrete even though its base `P` is declared.
        let p_of_t = registry.object(&JuliaType::Struct("P{T}".to_string()));
        assert!(!p_of_t.is_concrete_type());
    }
}
