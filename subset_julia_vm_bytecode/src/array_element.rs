//! Array element type definitions for homogeneous typed arrays.

use serde::{Deserialize, Serialize};

use subset_julia_vm_types::types::JuliaType;

/// Element type for arrays
/// Note: Copy removed to allow TupleOf(Vec<ArrayElementType>)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ArrayElementType {
    // Floating point types
    F32,
    #[default]
    F64,
    // Complex types: Complex{T} stored as interleaved T
    // Julia: Complex{Float32}, Complex{Float64}
    ComplexF32,
    ComplexF64,
    // Signed integer types
    I8,
    I16,
    I32,
    I64,
    // Issue #3557: Int128 has no dedicated `ArrayData` storage variant —
    // arrays use `ArrayData::Any` with this tag set as
    // `element_type_override` so `typeof(Int128[])` reports
    // `Vector{Int128}` instead of `Vector{Any}`.
    I128,
    // Unsigned integer types
    U8,
    U16,
    U32,
    U64,
    // Issue #3557: UInt128 mirrors I128 — boxed `Any` storage with this
    // tag for display only.
    U128,
    // Other types
    Bool,
    String,
    /// SubString{String}: marker variant that shares storage with `String`
    /// but displays as `SubString{String}` so `split`-returned vectors match
    /// Julia's `Vector{SubString{String}}` show form (Issue #3574). The VM
    /// has no separate substring runtime type; values are still `Value::Str`.
    /// This tag is only set via the `_substring_retag` builtin and read for
    /// display / `eltype` purposes.
    SubString,
    Char,
    Symbol,
    /// Nothing has no inline storage, but `Vector{Nothing}` must retain the
    /// concrete logical element type instead of degrading to `Vector{Any}`.
    Nothing,
    Struct,
    StructOf(usize),
    /// isbits struct with inline AoS storage
    /// (type_id, field_count) for get/set without struct_defs lookup
    StructInlineOf(usize, usize),
    Any,
    /// Homogeneous tuple array: stores field types for AoS layout
    /// Example: Tuple{Int64, Float64} -> TupleOf(vec![I64, F64])
    /// Storage: ArrayData::Any with interleaved fields [a1, b1, a2, b2, ...]
    TupleOf(Vec<ArrayElementType>),
    /// Heterogeneous element type rendered as `Union{...}`.
    /// Issue #3549: lets `[1, nothing, 2]` print as `Vector{Union{Nothing, Int64}}`
    /// instead of `Vector{Any}`. Storage uses `ArrayData::Any` (heterogeneous);
    /// the members are the structured union components (e.g.
    /// `[Nothing, Int64]`), rendered as `Union{...}` by `julia_type_name`.
    ///
    /// Issue #6720: structured `Vec<JuliaType>` replaces the former
    /// pre-rendered `String` body. The members carry types like
    /// `Nothing`/`Missing` that have no `ArrayElementType` storage variant,
    /// which is why `JuliaType` (not `ArrayElementType`) is the member type.
    /// An empty member list renders as `Union{}` and lowers to `Bottom`.
    /// The display order of the members is preserved verbatim; canonicalization
    /// (flatten / sort / dedup, Issue #5066) is applied only at the
    /// materialization boundaries (`array_element_type_to_julia_type` and the
    /// `compile/bridge.rs` lattice conversion).
    UnionOf(Vec<JuliaType>),
    /// Abstract element type with boxed `Any` storage and a logical element tag.
    /// This preserves collection widening results such as `Vector{Real}`.
    Abstract(String),
    /// Float16 has no dedicated inline `ArrayData` storage variant — arrays use
    /// `ArrayData::Any` boxed storage with this tag so `typeof(Float16[])` /
    /// `eltype` / `collect(<Float16 generator>)` report `Vector{Float16}`
    /// instead of `Vector{Any}` (Issue #9301). Mirrors the `I128`/`U128`
    /// pattern (boxed storage + concrete display/eltype tag).
    ///
    /// Declared LAST so existing bincode-serialized variant discriminants stay
    /// stable (append-only enum rule for cached bytecode / Base cache).
    F16,
    /// isbits immutable struct whose fields are ALL `Float64`, stored
    /// byte-contiguously as interleaved raw `f64` in `ArrayData::StructF64`
    /// (Issue #9198 S4). `(type_id, field_count)`, mirroring `StructInlineOf`
    /// but with UNboxed contiguous storage instead of `ArrayData::Any`
    /// (`Vec<Value>`). This is the general form of the `ComplexF64` interleaved
    /// layout for the S2/S3 SROA'd 2×f64 shape (`Complex{Float64}`, user
    /// `struct V2 x::Float64; y::Float64 end`); it makes `sizeof(Vector{T})`
    /// match upstream's `n * field_count * 8` and mirrors
    /// `jl_get_genericmemory_layout` case 1 (inline unboxed isbits eltype).
    ///
    /// Declared LAST (after `F16`) for the same append-only bincode discriminant
    /// stability reason documented on `F16` above: `ArrayElementType` is embedded
    /// in serialized `Instr` payloads, so a mid-enum insertion would misdecode
    /// existing Base/prelude caches.
    StructInlineF64(usize, usize),
    /// A logical boxed element type that must retain its structured Julia type
    /// graph. Unlike `Abstract(String)`, this variant never reconstructs
    /// UnionAll/RuntimeTypeVar identity from a display name (Issue #11236).
    ///
    /// Declared last to preserve all existing serialized discriminants. Adding
    /// it still requires the precompile cache version bump in the VM crate.
    Structured(Box<JuliaType>),
}

impl ArrayElementType {
    /// Check if this is a complex type (Complex{T})
    pub fn is_complex(&self) -> bool {
        matches!(
            self,
            ArrayElementType::ComplexF32 | ArrayElementType::ComplexF64
        )
    }

    /// Get the underlying scalar type for complex types
    /// Returns None for non-complex types
    pub fn complex_scalar_type(&self) -> Option<ArrayElementType> {
        match self {
            ArrayElementType::ComplexF32 => Some(ArrayElementType::F32),
            ArrayElementType::ComplexF64 => Some(ArrayElementType::F64),
            _ => None,
        }
    }

    /// Create a complex type from a scalar type
    /// Returns None if the scalar type doesn't support complex
    pub fn as_complex(scalar: ArrayElementType) -> Option<ArrayElementType> {
        match scalar {
            ArrayElementType::F32 => Some(ArrayElementType::ComplexF32),
            ArrayElementType::F64 => Some(ArrayElementType::ComplexF64),
            _ => None, // Could extend to I64, I32 etc. if needed
        }
    }

    /// Check if this is a tuple array type (TupleOf)
    pub fn is_tuple(&self) -> bool {
        matches!(self, ArrayElementType::TupleOf(_))
    }

    /// Get tuple field types if this is a TupleOf
    pub fn tuple_field_types(&self) -> Option<&Vec<ArrayElementType>> {
        match self {
            ArrayElementType::TupleOf(types) => Some(types),
            _ => None,
        }
    }

    /// Get the arity (number of fields) for tuple arrays
    pub fn tuple_arity(&self) -> Option<usize> {
        match self {
            ArrayElementType::TupleOf(types) => Some(types.len()),
            _ => None,
        }
    }

    /// Check if this type is isbits (can be stored inline)
    pub fn is_isbits(&self) -> bool {
        match self {
            ArrayElementType::F32
            | ArrayElementType::F64
            | ArrayElementType::I8
            | ArrayElementType::I16
            | ArrayElementType::I32
            | ArrayElementType::I64
            | ArrayElementType::U8
            | ArrayElementType::U16
            | ArrayElementType::U32
            | ArrayElementType::U64
            | ArrayElementType::Bool
            | ArrayElementType::Char
            | ArrayElementType::ComplexF32
            | ArrayElementType::ComplexF64 => true,
            // Issue #3557: I128/U128 are storage-backed by ArrayData::Any
            // (boxed Vec<Value>) so they are not stored inline like the
            // smaller fixed-width primitives. Returning `false` keeps the
            // boxing/storage logic on the existing `Any` path.
            ArrayElementType::I128 | ArrayElementType::U128 => false,
            // Float16 uses boxed `ArrayData::Any` storage (no dedicated inline
            // variant), mirroring I128/U128 (Issue #9301).
            ArrayElementType::F16 => false,
            ArrayElementType::TupleOf(fields) => fields.iter().all(|f| f.is_isbits()),
            ArrayElementType::StructInlineOf(_, _) | ArrayElementType::StructInlineF64(_, _) => {
                true
            }
            ArrayElementType::UnionOf(_)
            | ArrayElementType::Abstract(_)
            | ArrayElementType::Structured(_) => false,
            _ => false,
        }
    }

    /// Check if this is an inline struct array type (boxed `StructInlineOf` AoS
    /// or the contiguous-`f64` `StructInlineF64`, Issue #9198 S4). Both require
    /// their eltype tag to be carried as an `element_type_override`.
    pub fn is_struct_inline(&self) -> bool {
        matches!(
            self,
            ArrayElementType::StructInlineOf(_, _) | ArrayElementType::StructInlineF64(_, _)
        )
    }

    /// Whether this is specifically the contiguous all-`Float64` inline struct
    /// array (backed by `ArrayData::StructF64`), as opposed to the boxed
    /// `StructInlineOf` AoS layout (Issue #9198 S4).
    pub fn is_struct_inline_f64(&self) -> bool {
        matches!(self, ArrayElementType::StructInlineF64(_, _))
    }

    /// Get struct inline info (type_id, field_count) for either inline struct
    /// array layout (`StructInlineOf` boxed AoS or `StructInlineF64` contiguous).
    pub fn struct_inline_info(&self) -> Option<(usize, usize)> {
        match self {
            ArrayElementType::StructInlineOf(type_id, field_count)
            | ArrayElementType::StructInlineF64(type_id, field_count) => {
                Some((*type_id, *field_count))
            }
            _ => None,
        }
    }

    /// Convert to ValueType
    pub fn to_value_type(&self) -> crate::ValueType {
        match self {
            ArrayElementType::F16 => crate::ValueType::F16,
            ArrayElementType::F32 => crate::ValueType::F32,
            ArrayElementType::F64 => crate::ValueType::F64,
            ArrayElementType::ComplexF32 => crate::ValueType::ComplexF32,
            ArrayElementType::ComplexF64 => crate::ValueType::ComplexF64,
            ArrayElementType::I8 => crate::ValueType::I8,
            ArrayElementType::I16 => crate::ValueType::I16,
            ArrayElementType::I32 => crate::ValueType::I32,
            ArrayElementType::I64 => crate::ValueType::I64,
            ArrayElementType::I128 => crate::ValueType::I128,
            ArrayElementType::U8 => crate::ValueType::U8,
            ArrayElementType::U16 => crate::ValueType::U16,
            ArrayElementType::U32 => crate::ValueType::U32,
            ArrayElementType::U64 => crate::ValueType::U64,
            ArrayElementType::U128 => crate::ValueType::U128,
            ArrayElementType::Bool => crate::ValueType::Bool,
            ArrayElementType::String => crate::ValueType::Str,
            // SubString{String} shares the same runtime value type as String;
            // the variant exists purely for display tagging (Issue #3574).
            ArrayElementType::SubString => crate::ValueType::Str,
            ArrayElementType::Char => crate::ValueType::Char,
            ArrayElementType::Symbol => crate::ValueType::Symbol,
            ArrayElementType::Nothing => crate::ValueType::Nothing,
            ArrayElementType::Struct => crate::ValueType::Any,
            ArrayElementType::StructOf(id) => crate::ValueType::Struct(*id),
            ArrayElementType::StructInlineOf(id, _) => crate::ValueType::Struct(*id),
            ArrayElementType::StructInlineF64(id, _) => crate::ValueType::Struct(*id),
            ArrayElementType::Any => crate::ValueType::Any,
            ArrayElementType::TupleOf(_) => crate::ValueType::Tuple,
            ArrayElementType::UnionOf(_)
            | ArrayElementType::Abstract(_)
            | ArrayElementType::Structured(_) => crate::ValueType::Any,
        }
    }

    /// Create from ValueType
    pub fn from_value_type(vt: &crate::ValueType) -> Self {
        match vt {
            crate::ValueType::F16 => ArrayElementType::F16,
            crate::ValueType::F32 => ArrayElementType::F32,
            crate::ValueType::F64 => ArrayElementType::F64,
            crate::ValueType::ComplexF32 => ArrayElementType::ComplexF32,
            crate::ValueType::ComplexF64 => ArrayElementType::ComplexF64,
            crate::ValueType::I8 => ArrayElementType::I8,
            crate::ValueType::I16 => ArrayElementType::I16,
            crate::ValueType::I32 => ArrayElementType::I32,
            crate::ValueType::I64 => ArrayElementType::I64,
            crate::ValueType::I128 => ArrayElementType::I128,
            crate::ValueType::U8 => ArrayElementType::U8,
            crate::ValueType::U16 => ArrayElementType::U16,
            crate::ValueType::U32 => ArrayElementType::U32,
            crate::ValueType::U64 => ArrayElementType::U64,
            crate::ValueType::U128 => ArrayElementType::U128,
            crate::ValueType::Bool => ArrayElementType::Bool,
            crate::ValueType::Str => ArrayElementType::String,
            crate::ValueType::Char => ArrayElementType::Char,
            crate::ValueType::Symbol => ArrayElementType::Symbol,
            crate::ValueType::Nothing => ArrayElementType::Nothing,
            crate::ValueType::Struct(id) => ArrayElementType::StructOf(*id),
            crate::ValueType::Tuple => ArrayElementType::Any,
            _ => ArrayElementType::Any,
        }
    }
}

impl ArrayElementType {
    /// Get the Julia type name for this element type (for display purposes)
    /// E.g., I64 -> "Int64", F64 -> "Float64"
    pub fn julia_type_name(&self) -> String {
        match self {
            ArrayElementType::F16 => "Float16".to_string(),
            ArrayElementType::F32 => "Float32".to_string(),
            ArrayElementType::F64 => "Float64".to_string(),
            ArrayElementType::ComplexF32 => "Complex{Float32}".to_string(),
            ArrayElementType::ComplexF64 => "Complex{Float64}".to_string(),
            ArrayElementType::I8 => "Int8".to_string(),
            ArrayElementType::I16 => "Int16".to_string(),
            ArrayElementType::I32 => "Int32".to_string(),
            ArrayElementType::I64 => "Int64".to_string(),
            ArrayElementType::I128 => "Int128".to_string(),
            ArrayElementType::U8 => "UInt8".to_string(),
            ArrayElementType::U16 => "UInt16".to_string(),
            ArrayElementType::U32 => "UInt32".to_string(),
            ArrayElementType::U64 => "UInt64".to_string(),
            ArrayElementType::U128 => "UInt128".to_string(),
            ArrayElementType::Bool => "Bool".to_string(),
            ArrayElementType::String => "String".to_string(),
            // Display name for the SubString tag. Used by show/`eltype` so
            // `Vector{SubString{String}}` matches Julia's surface syntax.
            ArrayElementType::SubString => "SubString{String}".to_string(),
            ArrayElementType::Char => "Char".to_string(),
            ArrayElementType::Symbol => "Symbol".to_string(),
            ArrayElementType::Nothing => "Nothing".to_string(),
            ArrayElementType::Struct => "Any".to_string(),
            ArrayElementType::StructOf(_) => "Any".to_string(), // Struct name would need lookup
            ArrayElementType::StructInlineOf(_, _) => "Any".to_string(),
            // The concrete struct name is resolved from `type_id` at the VM
            // layer (reflection), matching `StructInlineOf`/`StructOf`.
            ArrayElementType::StructInlineF64(_, _) => "Any".to_string(),
            ArrayElementType::Any => "Any".to_string(),
            ArrayElementType::TupleOf(field_types) => {
                let type_names: Vec<String> =
                    field_types.iter().map(|t| t.julia_type_name()).collect();
                format!("Tuple{{{}}}", type_names.join(", "))
            }
            ArrayElementType::UnionOf(members) => {
                format!("Union{{{}}}", Self::union_body_string(members))
            }
            ArrayElementType::Abstract(name) => name.clone(),
            ArrayElementType::Structured(ty) => ty.name().into_owned(),
        }
    }

    /// Render structured union members as the comma-separated body that goes
    /// inside `Union{...}` (e.g. `[Nothing, Int64]` → `"Nothing, Int64"`),
    /// preserving member order (Issue #6720).
    pub fn union_body_string(members: &[JuliaType]) -> String {
        members
            .iter()
            .map(|m| m.name().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Build a `UnionOf` element type from a `Union{...}` *body* string (the
    /// comma-separated member list without the `Union{}` wrapper, e.g.
    /// `"Nothing, Int64"`), lifting it into structured `JuliaType` members
    /// (Issue #6720). Member order is preserved; an empty/blank body yields an
    /// empty member list (renders `Union{}`, lowers to `Bottom`). Commas nested
    /// inside `{...}` are not split, so parametric members such as
    /// `Pair{Int64, String}` survive intact.
    pub fn union_from_body(body: &str) -> ArrayElementType {
        let members = split_top_level_commas(body)
            .iter()
            .map(|m| m.trim())
            .filter(|m| !m.is_empty())
            .map(JuliaType::from_name_or_struct)
            .collect();
        ArrayElementType::UnionOf(members)
    }

    /// Whether this element type is "implicit" for array-show purposes, mirroring
    /// upstream Julia's `typeinfo_implicit` (`julia/base/arrayshow.jl`). An array
    /// whose eltype is implicit prints WITHOUT a `T[...]` type prefix
    /// (e.g. `[1, 2]`, `[1.0, 2.0]`, `['a', 'b']`); a non-implicit eltype prints
    /// the prefix (e.g. `Int8[1, 2]`, `Float32[1.0, 2.0]`, `Bool[1, 0]`).
    ///
    /// Implicit types are those that can be parsed back accurately from their
    /// un-decorated representation: `Int64`, `Float64`, `Char`, `String`,
    /// `Symbol`, and concrete `Tuple` whose field types are all implicit
    /// (`Pair` is handled at the value level by `array_show_prefix`, since the
    /// `Pair` eltype carries no dedicated `ArrayElementType` tag here).
    /// Note: `Struct`/`StructOf`/`StructInlineOf`/`Any` are reported as
    /// non-implicit; the formatter derives the concrete prefix from the
    /// element values for those tags (see `array_show_prefix`).
    pub fn typeinfo_implicit(&self) -> bool {
        match self {
            ArrayElementType::I64
            | ArrayElementType::F64
            | ArrayElementType::Char
            | ArrayElementType::String
            | ArrayElementType::Symbol => true,
            ArrayElementType::TupleOf(fields) => fields.iter().all(|f| f.typeinfo_implicit()),
            // A concrete parametric eltype stored as `Abstract` (Issue #6768)
            // follows upstream's recursive `typeinfo_implicit`: an
            // `Array{T,N}` / `Vector{T}` / `Matrix{T}` element type is implicit
            // iff its inner eltype `T` is implicit, so `Vector{Vector{Int64}}`
            // prints bare `[[1, 2]]` while `Vector{UnitRange{Int64}}` keeps its
            // `UnitRange{Int64}[...]` prefix.
            ArrayElementType::Abstract(name) => type_name_typeinfo_implicit(name),
            ArrayElementType::Structured(_) => false,
            _ => false,
        }
    }
}

/// Mirror upstream Julia's `typeinfo_implicit` (`julia/base/arrayshow.jl`) for a
/// type *name* string. Implicit scalar leaves print without a `T[...]` prefix;
/// nested `Array`/`Vector`/`Matrix` recurse on the inner eltype; concrete
/// `Tuple{...}` is implicit iff every component is. Everything else (including
/// `UnitRange{Int64}`) is non-implicit. Used for the `Abstract` element tag.
fn type_name_typeinfo_implicit(name: &str) -> bool {
    let name = name.trim();
    match name {
        "Int64" | "Float64" | "Char" | "String" | "Symbol" => return true,
        // `Int` is the platform word alias; it is implicit when it denotes
        // `Int64` (the 64-bit target). `UInt`/`UInt64` are non-implicit.
        "Int" if subset_julia_vm_types::types::native_int_type_name() == "Int64" => return true,
        _ => {}
    }
    let Some((base, inner)) = split_parametric_name(name) else {
        return false;
    };
    match base {
        "Vector" | "Array" | "Matrix" => {
            // `Array{T, N}` carries a trailing rank parameter; the element type
            // is the first comma-separated component. `Vector{T}` / `Matrix{T}`
            // have just the element type.
            let elem = split_top_level_commas(inner)
                .into_iter()
                .next()
                .unwrap_or(inner);
            type_name_typeinfo_implicit(elem.trim())
        }
        "Tuple" | "Pair" => split_top_level_commas(inner)
            .iter()
            .all(|component| type_name_typeinfo_implicit(component.trim())),
        "NamedTuple" => {
            let params = split_top_level_commas(inner);
            matches!(
                params.as_slice(),
                [_, field_tuple] if type_name_typeinfo_implicit(field_tuple.trim())
            )
        }
        "@NamedTuple" => {
            let inner = inner.trim();
            inner.is_empty()
                || split_top_level_commas(inner).iter().all(|component| {
                    component
                        .split_once("::")
                        .is_some_and(|(_, ty)| type_name_typeinfo_implicit(ty.trim()))
                })
        }
        "Dict" => {
            let params = split_top_level_commas(inner);
            matches!(
                params.as_slice(),
                [key, val] if type_name_typeinfo_implicit(key.trim())
                    && type_name_typeinfo_implicit(val.trim())
            )
        }
        _ => false,
    }
}

/// Split `Base{inner}` into `(Base, inner)`. Returns `None` when `name` is not a
/// well-formed parametric type-name string.
fn split_parametric_name(name: &str) -> Option<(&str, &str)> {
    let open = name.find('{')?;
    if !name.ends_with('}') {
        return None;
    }
    let base = &name[..open];
    let inner = &name[open + 1..name.len() - 1];
    Some((base, inner))
}

/// Split a parameter list on top-level commas, ignoring commas nested inside
/// `{...}` / `(...)` (so `NamedTuple{(:a, :b), Tuple{Int64, String}}` splits
/// into the two intended parts).
fn split_top_level_commas(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut brace_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            ',' if brace_depth == 0 && paren_depth == 0 => {
                parts.push(&inner[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    parts
}

/// Construct the Julia array type projection for an element type and rank.
///
/// Julia defines `Vector{T}` and `Matrix{T}` as aliases of `Array{T,1}` and
/// `Array{T,2}` respectively. Higher-dimensional arrays keep the explicit
/// `Array{T,N}` form.
pub fn julia_array_type_for_ndims(elem_type: JuliaType, ndims: usize) -> JuliaType {
    match ndims {
        1 => JuliaType::VectorOf(Box::new(elem_type)),
        2 => JuliaType::MatrixOf(Box::new(elem_type)),
        n if elem_type.contains_runtime_typevar()
            || matches!(
                &elem_type,
                JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. }
            ) =>
        {
            JuliaType::RuntimeParametric {
                base: "Array".to_string(),
                params: vec![elem_type, JuliaType::Struct(n.to_string())],
            }
        }
        n => JuliaType::Struct(format!("Array{{{}, {}}}", elem_type.name(), n)),
    }
}

pub fn array_element_type_to_julia_type(element_type: &ArrayElementType) -> JuliaType {
    match element_type {
        ArrayElementType::F16 => JuliaType::Float16,
        ArrayElementType::F32 => JuliaType::Float32,
        ArrayElementType::F64 => JuliaType::Float64,
        ArrayElementType::ComplexF32 => JuliaType::Struct("Complex{Float32}".to_string()),
        ArrayElementType::ComplexF64 => JuliaType::Struct("Complex{Float64}".to_string()),
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
        ArrayElementType::Bool => JuliaType::Bool,
        ArrayElementType::String => JuliaType::String,
        ArrayElementType::SubString => JuliaType::Struct("SubString{String}".to_string()),
        ArrayElementType::Char => JuliaType::Char,
        ArrayElementType::Symbol => JuliaType::Symbol,
        ArrayElementType::Nothing => JuliaType::Nothing,
        ArrayElementType::TupleOf(field_types) => JuliaType::TupleOf(
            field_types
                .iter()
                .map(array_element_type_to_julia_type)
                .collect(),
        ),
        ArrayElementType::UnionOf(members) if members.is_empty() => JuliaType::Bottom,
        // Issue #5335: materialize the element type as a real `JuliaType::Union`
        // rather than a `Struct("Union{...}")` name. The struct form tagged the
        // value as a DataType and never compared `==` to an equivalent
        // `Union{...}` literal.
        // Issue #6720: members are already structured, so canonicalize them
        // directly (flatten / dedup / sort / collapse, Issue #5066) instead of
        // rendering to a string and re-parsing via `from_name_or_struct`. This
        // is byte-identical to the old round-trip because the parser delegates
        // to the same `canonicalize_union`.
        ArrayElementType::UnionOf(members) => {
            subset_julia_vm_types::types::canonicalize_union(members.clone())
        }
        ArrayElementType::Abstract(name) => JuliaType::from_name_or_struct(name),
        ArrayElementType::Structured(ty) => ty.as_ref().clone(),
        ArrayElementType::Struct
        | ArrayElementType::StructOf(_)
        | ArrayElementType::StructInlineOf(_, _)
        | ArrayElementType::StructInlineF64(_, _)
        | ArrayElementType::Any => JuliaType::Any,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_complex ────────────────────────────────────────────────────────────

    #[test]
    fn test_is_complex_for_complex_variants() {
        assert!(ArrayElementType::ComplexF32.is_complex());
        assert!(ArrayElementType::ComplexF64.is_complex());
    }

    #[test]
    fn test_is_complex_false_for_scalar_variants() {
        assert!(!ArrayElementType::F64.is_complex());
        assert!(!ArrayElementType::I64.is_complex());
        assert!(!ArrayElementType::Any.is_complex());
    }

    // ── complex_scalar_type ───────────────────────────────────────────────────

    #[test]
    fn test_complex_scalar_type_complex_f64_returns_f64() {
        assert_eq!(
            ArrayElementType::ComplexF64.complex_scalar_type(),
            Some(ArrayElementType::F64)
        );
    }

    #[test]
    fn test_complex_scalar_type_complex_f32_returns_f32() {
        assert_eq!(
            ArrayElementType::ComplexF32.complex_scalar_type(),
            Some(ArrayElementType::F32)
        );
    }

    #[test]
    fn test_complex_scalar_type_returns_none_for_non_complex() {
        assert_eq!(ArrayElementType::F64.complex_scalar_type(), None);
        assert_eq!(ArrayElementType::I64.complex_scalar_type(), None);
    }

    // ── as_complex ────────────────────────────────────────────────────────────

    #[test]
    fn test_as_complex_f64_returns_complex_f64() {
        assert_eq!(
            ArrayElementType::as_complex(ArrayElementType::F64),
            Some(ArrayElementType::ComplexF64)
        );
    }

    #[test]
    fn test_as_complex_f32_returns_complex_f32() {
        assert_eq!(
            ArrayElementType::as_complex(ArrayElementType::F32),
            Some(ArrayElementType::ComplexF32)
        );
    }

    #[test]
    fn test_as_complex_integer_returns_none() {
        assert_eq!(ArrayElementType::as_complex(ArrayElementType::I64), None);
    }

    // ── is_tuple / tuple_arity / tuple_field_types ────────────────────────────

    #[test]
    fn test_is_tuple_for_tuple_of() {
        let t = ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::F64]);
        assert!(t.is_tuple());
    }

    #[test]
    fn test_is_tuple_false_for_non_tuple() {
        assert!(!ArrayElementType::F64.is_tuple());
        assert!(!ArrayElementType::Any.is_tuple());
    }

    #[test]
    fn test_tuple_arity_returns_field_count() {
        let t = ArrayElementType::TupleOf(vec![
            ArrayElementType::I64,
            ArrayElementType::F64,
            ArrayElementType::Bool,
        ]);
        assert_eq!(t.tuple_arity(), Some(3));
    }

    #[test]
    fn test_tuple_arity_none_for_non_tuple() {
        assert_eq!(ArrayElementType::F64.tuple_arity(), None);
    }

    #[test]
    fn test_tuple_field_types_returns_inner_vec() {
        let t = ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::F64]);
        let fields = t.tuple_field_types().unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0], ArrayElementType::I64);
        assert_eq!(fields[1], ArrayElementType::F64);
    }

    // ── is_isbits ─────────────────────────────────────────────────────────────

    #[test]
    fn test_is_isbits_for_primitives() {
        assert!(ArrayElementType::F64.is_isbits());
        assert!(ArrayElementType::I64.is_isbits());
        assert!(ArrayElementType::Bool.is_isbits());
        assert!(ArrayElementType::ComplexF64.is_isbits());
    }

    #[test]
    fn test_is_isbits_false_for_heap_types() {
        assert!(!ArrayElementType::String.is_isbits());
        assert!(!ArrayElementType::Any.is_isbits());
        assert!(!ArrayElementType::Struct.is_isbits());
    }

    #[test]
    fn test_is_isbits_for_tuple_of_primitives() {
        let t = ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::F64]);
        assert!(t.is_isbits(), "TupleOf(I64, F64) should be isbits");
    }

    #[test]
    fn test_is_isbits_false_for_tuple_containing_non_isbits() {
        let t = ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::String]);
        assert!(!t.is_isbits(), "TupleOf(I64, String) should NOT be isbits");
    }

    // ── julia_type_name ───────────────────────────────────────────────────────

    #[test]
    fn test_julia_type_name_primitives() {
        assert_eq!(ArrayElementType::F64.julia_type_name(), "Float64");
        assert_eq!(ArrayElementType::I64.julia_type_name(), "Int64");
        assert_eq!(ArrayElementType::Bool.julia_type_name(), "Bool");
        assert_eq!(ArrayElementType::U8.julia_type_name(), "UInt8");
    }

    #[test]
    fn test_julia_type_name_complex() {
        assert_eq!(
            ArrayElementType::ComplexF64.julia_type_name(),
            "Complex{Float64}"
        );
        assert_eq!(
            ArrayElementType::ComplexF32.julia_type_name(),
            "Complex{Float32}"
        );
    }

    #[test]
    fn test_julia_type_name_tuple_of() {
        let t = ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::F64]);
        assert_eq!(t.julia_type_name(), "Tuple{Int64, Float64}");
    }

    #[test]
    fn test_julia_type_name_any_variants() {
        assert_eq!(ArrayElementType::Any.julia_type_name(), "Any");
        assert_eq!(ArrayElementType::Struct.julia_type_name(), "Any");
    }

    // ── typeinfo_implicit for Abstract parametric eltypes (Issue #6768) ───────

    #[test]
    fn test_typeinfo_implicit_unitrange_is_non_implicit_6768() {
        // UnitRange{Int64} is NOT in upstream's implicit set, so a
        // `Vector{UnitRange{Int64}}` prints the `UnitRange{Int64}[...]` prefix.
        assert!(!ArrayElementType::Abstract("UnitRange{Int64}".to_string()).typeinfo_implicit());
    }

    #[test]
    fn test_typeinfo_implicit_nested_vector_recurses_6768() {
        // Array{T,N} of an implicit eltype is implicit (prints bare).
        assert!(ArrayElementType::Abstract("Vector{Int64}".to_string()).typeinfo_implicit());
        assert!(ArrayElementType::Abstract("Vector{Int}".to_string()).typeinfo_implicit());
        assert!(ArrayElementType::Abstract("Vector{Float64}".to_string()).typeinfo_implicit());
        assert!(ArrayElementType::Abstract("Array{Int64, 2}".to_string()).typeinfo_implicit());
        assert!(ArrayElementType::Abstract("Matrix{Int64}".to_string()).typeinfo_implicit());
        // Non-implicit inner eltype keeps the array eltype non-implicit.
        assert!(!ArrayElementType::Abstract("Vector{Int8}".to_string()).typeinfo_implicit());
        assert!(
            !ArrayElementType::Abstract("Vector{UnitRange{Int64}}".to_string()).typeinfo_implicit()
        );
    }

    #[test]
    fn test_typeinfo_implicit_tuple_name_recurses_6768() {
        assert!(
            ArrayElementType::Abstract("Tuple{Int64, Float64}".to_string()).typeinfo_implicit()
        );
        assert!(!ArrayElementType::Abstract("Tuple{Int64, Int8}".to_string()).typeinfo_implicit());
    }

    #[test]
    fn test_split_top_level_commas_ignores_nested_braces() {
        assert_eq!(
            split_top_level_commas("Pair{Int64, String}, Int64"),
            vec!["Pair{Int64, String}", " Int64"]
        );
        assert_eq!(split_top_level_commas("Int64"), vec!["Int64"]);
    }

    // ── UnionOf structured payload (Issue #6720) ──────────────────────────────

    #[test]
    fn union_from_body_parses_structured_members_issue_6720() {
        // The element-type union body is lifted from a string into structured
        // `JuliaType` members (no more `UnionOf(String)`), preserving order.
        assert_eq!(
            ArrayElementType::union_from_body("Nothing, Int64"),
            ArrayElementType::UnionOf(vec![JuliaType::Nothing, JuliaType::Int64])
        );
        // Empty body -> empty member list (renders `Union{}`, lowers to Bottom).
        assert_eq!(
            ArrayElementType::union_from_body(""),
            ArrayElementType::UnionOf(Vec::new())
        );
        // Brace-aware: a parametric member is not split on its inner comma.
        assert_eq!(
            ArrayElementType::union_from_body("Nothing, Pair{Int64, String}"),
            ArrayElementType::UnionOf(vec![
                JuliaType::Nothing,
                JuliaType::from_name_or_struct("Pair{Int64, String}"),
            ])
        );
    }

    #[test]
    fn union_of_display_preserves_member_order_issue_6720() {
        let u = ArrayElementType::UnionOf(vec![JuliaType::Nothing, JuliaType::Int64]);
        assert_eq!(u.julia_type_name(), "Union{Nothing, Int64}");
        let empty = ArrayElementType::UnionOf(Vec::new());
        assert_eq!(empty.julia_type_name(), "Union{}");
    }

    #[test]
    fn union_of_materializes_canonical_julia_type_issue_6720() {
        // Materialization canonicalizes (Issue #5066) like the old string
        // round-trip did, but now structurally without re-parsing a string.
        let u = ArrayElementType::UnionOf(vec![JuliaType::Nothing, JuliaType::Int64]);
        assert_eq!(
            array_element_type_to_julia_type(&u),
            JuliaType::from_name_or_struct("Union{Nothing, Int64}")
        );
        let empty = ArrayElementType::UnionOf(Vec::new());
        assert_eq!(array_element_type_to_julia_type(&empty), JuliaType::Bottom);
    }

    #[test]
    fn structured_element_type_preserves_runtime_typevar_identity_issue_11236() {
        let var = JuliaType::RuntimeTypeVar {
            id: 11236,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let partial_family = JuliaType::RuntimeUnionAll {
            var: Box::new(var.clone()),
            body: Box::new(JuliaType::RuntimeParametric {
                base: "SubArray".to_string(),
                params: vec![JuliaType::Int8, var],
            }),
        };
        let element = ArrayElementType::Structured(Box::new(partial_family.clone()));

        assert_eq!(
            array_element_type_to_julia_type(&element),
            partial_family,
            "array eltype materialization must not render and reparse runtime TypeVar ids"
        );
        assert_eq!(element.to_value_type(), crate::ValueType::Any);

        let rank_three = julia_array_type_for_ndims(partial_family.clone(), 3);
        assert_eq!(
            rank_three,
            JuliaType::RuntimeParametric {
                base: "Array".to_string(),
                params: vec![partial_family, JuliaType::Struct("3".to_string())],
            },
            "rank >= 3 array materialization must not render the structured eltype"
        );
    }

    // ── StructInlineF64 (Issue #9198 S4) ──────────────────────────────────────

    #[test]
    fn test_struct_inline_f64_predicates_issue_9198() {
        let t = ArrayElementType::StructInlineF64(7, 2);
        assert!(t.is_isbits(), "all-Float64 isbits struct is isbits");
        assert!(t.is_struct_inline());
        assert!(t.is_struct_inline_f64());
        assert!(!t.is_complex());
        assert!(!t.is_tuple());
        assert_eq!(t.struct_inline_info(), Some((7, 2)));
        // Reuses the same reflection story as StructInlineOf/StructOf: the
        // concrete name is resolved from `type_id` at the VM layer.
        assert_eq!(t.julia_type_name(), "Any");
        assert!(!t.typeinfo_implicit());
    }

    #[test]
    fn test_struct_inline_f64_type_mappings_issue_9198() {
        use crate::ValueType;
        let t = ArrayElementType::StructInlineF64(5, 3);
        assert_eq!(t.to_value_type(), ValueType::Struct(5));
        assert_eq!(array_element_type_to_julia_type(&t), JuliaType::Any);
    }

    #[test]
    fn nothing_element_type_round_trips_issue_8387() {
        assert_eq!(ArrayElementType::Nothing.julia_type_name(), "Nothing");
        assert_eq!(
            array_element_type_to_julia_type(&ArrayElementType::Nothing),
            JuliaType::Nothing
        );
        assert_eq!(
            ArrayElementType::from_value_type(&crate::ValueType::Nothing),
            ArrayElementType::Nothing
        );
    }

    // ── to_value_type (Issue #6919) ──────────────────────────────────────────

    /// Issue #6919 (epic #5916, `ValueType` demotion continuation): pin the
    /// canonical `ArrayElementType → ValueType` mapping on `to_value_type` over
    /// *every* variant. The compiler-side `value_type_from_array_element_type`
    /// (`vm/specialize/expr.rs`) was a byte-identical duplicate of this table;
    /// it now delegates here, so this is the single source of truth and a
    /// future divergence is caught.
    #[test]
    fn to_value_type_maps_every_variant_issue_6919() {
        use crate::ValueType;
        let cases: &[(ArrayElementType, ValueType)] = &[
            (ArrayElementType::F32, ValueType::F32),
            (ArrayElementType::F64, ValueType::F64),
            (ArrayElementType::ComplexF32, ValueType::ComplexF32),
            (ArrayElementType::ComplexF64, ValueType::ComplexF64),
            (ArrayElementType::I8, ValueType::I8),
            (ArrayElementType::I16, ValueType::I16),
            (ArrayElementType::I32, ValueType::I32),
            (ArrayElementType::I64, ValueType::I64),
            (ArrayElementType::I128, ValueType::I128),
            (ArrayElementType::U8, ValueType::U8),
            (ArrayElementType::U16, ValueType::U16),
            (ArrayElementType::U32, ValueType::U32),
            (ArrayElementType::U64, ValueType::U64),
            (ArrayElementType::U128, ValueType::U128),
            (ArrayElementType::Bool, ValueType::Bool),
            (ArrayElementType::String, ValueType::Str),
            // SubString shares the String runtime value type (Issue #3574).
            (ArrayElementType::SubString, ValueType::Str),
            (ArrayElementType::Char, ValueType::Char),
            (ArrayElementType::Symbol, ValueType::Symbol),
            (ArrayElementType::Nothing, ValueType::Nothing),
            (ArrayElementType::Struct, ValueType::Any),
            (ArrayElementType::StructOf(7), ValueType::Struct(7)),
            (ArrayElementType::StructInlineOf(7, 3), ValueType::Struct(7)),
            (ArrayElementType::Any, ValueType::Any),
            (
                ArrayElementType::TupleOf(vec![ArrayElementType::I64, ArrayElementType::F64]),
                ValueType::Tuple,
            ),
            (
                ArrayElementType::UnionOf(vec![JuliaType::Nothing, JuliaType::Int64]),
                ValueType::Any,
            ),
            (
                ArrayElementType::Abstract("Real".to_string()),
                ValueType::Any,
            ),
            (
                ArrayElementType::Structured(Box::new(JuliaType::RuntimeTypeVar {
                    id: 11236,
                    name: "T".to_string(),
                    lower_bound: Box::new(JuliaType::Bottom),
                    upper_bound: Box::new(JuliaType::Any),
                })),
                ValueType::Any,
            ),
        ];
        for (elem, expected) in cases {
            assert_eq!(
                elem.to_value_type(),
                *expected,
                "{elem:?} must map to {expected:?}"
            );
        }
    }
}

/// Bare-identifier element type for a `T[...]` typed array literal, combining
/// the builtin scalar map and the heap-container map. Lives in the shared
/// bytecode crate because BOTH codegen paths — the main compiler's `getindex`
/// literal arm and the VM runtime specializer's literal build (Issue #10746) —
/// must agree on the storage tag (same pattern as `typed_scalar_binary_instr`,
/// Issue #8192).
pub fn bare_type_name_array_element_type(name: &str) -> Option<ArrayElementType> {
    builtin_type_array_element_type(name).or_else(|| heap_type_array_element_type(name))
}

pub fn builtin_type_array_element_type(name: &str) -> Option<ArrayElementType> {
    match name {
        "Int" if subset_julia_vm_types::types::native_int_type_name() == "Int32" => {
            Some(ArrayElementType::I32)
        }
        "Int" | "Int64" => Some(ArrayElementType::I64),
        "Int32" => Some(ArrayElementType::I32),
        "Int16" => Some(ArrayElementType::I16),
        "Int8" => Some(ArrayElementType::I8),
        "Int128" => Some(ArrayElementType::I128),
        "UInt" if subset_julia_vm_types::types::native_uint_type_name() == "UInt32" => {
            Some(ArrayElementType::U32)
        }
        "UInt64" | "UInt" => Some(ArrayElementType::U64),
        "UInt32" => Some(ArrayElementType::U32),
        "UInt16" => Some(ArrayElementType::U16),
        "UInt8" => Some(ArrayElementType::U8),
        "UInt128" => Some(ArrayElementType::U128),
        "Float64" => Some(ArrayElementType::F64),
        "Float32" => Some(ArrayElementType::F32),
        // Issue #9301: Float16 typed literal `Float16[...]` resolves to F16 so
        // the shared typed-literal builder calls `convert(Float16, x)`, matching
        // Float32/Float64.
        "Float16" => Some(ArrayElementType::F16),
        // The `ComplexFNN` aliases name the same eltype as the parametric
        // `Complex{FloatNN}` spelling; without these a bare-identifier typed
        // literal `ComplexF64[...]` fell through to `Any` while
        // `Complex{Float64}[...]` resolved correctly (Issue #5775).
        "ComplexF64" => Some(ArrayElementType::ComplexF64),
        "ComplexF32" => Some(ArrayElementType::ComplexF32),
        "Number" => Some(ArrayElementType::Abstract("Number".to_string())),
        "Real" => Some(ArrayElementType::Abstract("Real".to_string())),
        "Integer" => Some(ArrayElementType::Abstract("Integer".to_string())),
        "Signed" => Some(ArrayElementType::Abstract("Signed".to_string())),
        "Unsigned" => Some(ArrayElementType::Abstract("Unsigned".to_string())),
        "AbstractFloat" => Some(ArrayElementType::Abstract("AbstractFloat".to_string())),
        "Bool" => Some(ArrayElementType::Bool),
        "String" => Some(ArrayElementType::String),
        "Char" => Some(ArrayElementType::Char),
        "Symbol" => Some(ArrayElementType::Symbol),
        "Any" => Some(ArrayElementType::Any),
        _ => None,
    }
}

fn heap_type_array_element_type(name: &str) -> Option<ArrayElementType> {
    match name {
        "Pair" => Some(ArrayElementType::Abstract("Pair".to_string())),
        "Dict" | "Set" => Some(ArrayElementType::Any),
        // Regex / RegexMatch are boxed scalar values (`Value::Regex` /
        // `Value::RegexMatch`); a typed literal `Regex[...]` stores them verbatim in
        // a boxed slot tagged with the type name, so `eltype` reports `Regex`. Without
        // this, the literal fell through to the numeric default and tried to coerce
        // each element to `I64` ("Cannot convert Regex to I64", Issue #5706).
        "Regex" => Some(ArrayElementType::Abstract("Regex".to_string())),
        "RegexMatch" => Some(ArrayElementType::Abstract("RegexMatch".to_string())),
        _ => None,
    }
}
