//! Bridge between VM runtime types and compile-time lattice types.
//!
//! This module provides bidirectional conversions between:
//! - `ValueType` (VM runtime type system)
//! - `LatticeType` (compile-time abstract interpretation type system)
//!
//! The conversions enable type inference results to be used for optimization
//! and allow runtime type information to inform compile-time analysis.

use crate::compile::context::StructInfo;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::inference_core::CorePrimitive;
use crate::inference_core::{CoreAbstract, CoreType};
use crate::types::JuliaType;
use crate::vm::value::{ArrayElementType, ValueType};
use std::collections::HashMap;

/// Single source of truth for the nullary-primitive `CorePrimitive → ValueType`
/// mapping (Issue #6916, epic #5916). The `LatticeType → ValueType` reverse
/// bridge below delegates here so its per-primitive arms collapse to a single
/// `Core(Primitive(p)) => ValueType::from(p)`. The match is exhaustive over
/// `CorePrimitive`, so adding a primitive is a compile error here rather than a
/// silent miss. Pinned by
/// `value_type_from_core_primitive_is_reverse_bridge_source_of_truth_issue_6916`.
impl From<&CorePrimitive> for ValueType {
    fn from(primitive: &CorePrimitive) -> Self {
        match primitive {
            CorePrimitive::Int8 => ValueType::I8,
            CorePrimitive::Int16 => ValueType::I16,
            CorePrimitive::Int32 => ValueType::I32,
            CorePrimitive::Int64 => ValueType::I64,
            CorePrimitive::Int128 => ValueType::I128,
            CorePrimitive::BigInt => ValueType::BigInt,
            CorePrimitive::UInt8 => ValueType::U8,
            CorePrimitive::UInt16 => ValueType::U16,
            CorePrimitive::UInt32 => ValueType::U32,
            CorePrimitive::UInt64 => ValueType::U64,
            CorePrimitive::UInt128 => ValueType::U128,
            CorePrimitive::Bool => ValueType::Bool,
            CorePrimitive::Float16 => ValueType::F16,
            CorePrimitive::Float32 => ValueType::F32,
            CorePrimitive::Float64 => ValueType::F64,
            CorePrimitive::BigFloat => ValueType::BigFloat,
            CorePrimitive::String => ValueType::Str,
            CorePrimitive::Char => ValueType::Char,
            CorePrimitive::Nothing => ValueType::Nothing,
            CorePrimitive::Missing => ValueType::Missing,
            CorePrimitive::Symbol => ValueType::Symbol,
        }
    }
}

/// Convert a VM `ValueType` to a compile-time `LatticeType`.
///
/// This conversion is used when runtime type information needs to inform
/// compile-time type inference or optimization decisions.
impl From<&ValueType> for LatticeType {
    fn from(value_type: &ValueType) -> Self {
        match value_type {
            // Integer types - signed
            ValueType::I8 => {
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)))
            }
            ValueType::I16 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int16,
            ))),
            ValueType::I32 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int32,
            ))),
            ValueType::I64 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            ValueType::I128 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int128,
            ))),
            ValueType::BigInt => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt,
            ))),

            // Integer types - unsigned
            ValueType::U8 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt8,
            ))),
            ValueType::U16 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt16,
            ))),
            ValueType::U32 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt32,
            ))),
            ValueType::U64 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt64,
            ))),
            ValueType::U128 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt128,
            ))),

            // Boolean
            ValueType::Bool => {
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
            }

            // Floating point types
            ValueType::F16 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float16,
            ))),
            ValueType::F32 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32,
            ))),
            ValueType::F64 => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
            ValueType::BigFloat => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat,
            ))),

            // Array types
            ValueType::Array => {
                // Unknown element type.
                LatticeType::Concrete(ConcreteType::array(ConcreteType::Core(CoreType::Any), None))
            }
            ValueType::ArrayOf(elem_type, ndims) => {
                let element = convert_array_element_type(elem_type);
                LatticeType::Concrete(ConcreteType::array(element, *ndims))
            }

            // String types
            ValueType::Str => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
            ValueType::Char => {
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
            }

            // Special types
            ValueType::Nothing => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing,
            ))),
            ValueType::Missing => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Missing,
            ))),

            // Symbolic types
            ValueType::Symbol => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Symbol,
            ))),

            // Struct types
            ValueType::ComplexF32 => {
                LatticeType::Concrete(ConcreteType::struct_named("Complex{Float32}"))
            }
            ValueType::ComplexF64 => {
                LatticeType::Concrete(ConcreteType::struct_named("Complex{Float64}"))
            }
            ValueType::Struct(type_id) => LatticeType::Concrete(ConcreteType::struct_with_id(
                format!("Struct#{}", type_id),
                *type_id,
            )),

            // Tuple and NamedTuple - fallback to generic representation
            // (unknown element types / fields).
            ValueType::Tuple => LatticeType::Concrete(ConcreteType::tuple(vec![])),
            ValueType::NamedTuple => LatticeType::Concrete(ConcreteType::named_tuple(vec![])),

            // Range types — default to Int64 for ranges.
            ValueType::Range | ValueType::Rng => LatticeType::Concrete(ConcreteType::range(
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            )),

            // Dictionary type
            // A bare `ValueType::Dict` carries no element-type information, so its
            // key/value types are unknown — default both to `Any`, matching how
            // `ValueType::Array` defaults its element type. Defaulting the value to
            // `Float64` here made `get!`/`getindex` (whose tfuncs return the dict's
            // value type) infer `Float64` for any untyped Dict, spuriously coercing a
            // correct Int/String result to Float (Issue #6585).
            ValueType::Dict => LatticeType::Concrete(ConcreteType::dict(
                ConcreteType::Core(CoreType::Any),
                ConcreteType::Core(CoreType::Any),
            )),

            // Set type — default element type.
            ValueType::Set => LatticeType::Concrete(ConcreteType::set(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64),
            ))),

            // Generator type — default element type.
            ValueType::Generator => LatticeType::Concrete(ConcreteType::generator(
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            )),

            // Pairs type
            ValueType::Pairs => LatticeType::Concrete(ConcreteType::Pairs),

            // Type system types
            ValueType::DataType => LatticeType::Concrete(ConcreteType::data_type("DataType")),
            ValueType::Module => LatticeType::Concrete(ConcreteType::module_named("Module")),

            // IO type
            ValueType::IO => {
                LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::IO)))
            }

            // Function type
            ValueType::Function => LatticeType::Concrete(ConcreteType::Function {
                name: "Function".to_string(),
            }),

            // Metaprogramming types
            ValueType::Expr => LatticeType::Concrete(ConcreteType::Expr),
            ValueType::QuoteNode => LatticeType::Concrete(ConcreteType::QuoteNode),
            ValueType::LineNumberNode => LatticeType::Concrete(ConcreteType::LineNumberNode),
            ValueType::GlobalRef => LatticeType::Concrete(ConcreteType::GlobalRef),

            // Regex types
            ValueType::Regex => LatticeType::Concrete(ConcreteType::Regex),
            ValueType::RegexMatch => LatticeType::Concrete(ConcreteType::RegexMatch),

            // Enum type — maps to dedicated ConcreteType::Enum (Issue #2863)
            ValueType::Enum => LatticeType::Concrete(ConcreteType::Enum {
                name: "Enum".to_string(),
            }),

            // Union type - convert back to LatticeType::Union
            ValueType::Union(types) => {
                // `Union{}` (the empty union) is Julia's Bottom type. The VM
                // uses `ValueType::Union(vec![])` as its `Union{}` carrier
                // (see `julia_type_to_value_type`: `JuliaType::Bottom` maps to
                // `Union(Vec::new())`), so map it back to `LatticeType::Bottom`
                // instead of widening to `Top`. Without this, `Union{}`
                // entering through `ValueType` inverted the lattice (the most
                // specific type came back as the most general) — Issue #5916.
                if types.is_empty() {
                    return LatticeType::Bottom;
                }
                let concrete_types: Vec<ConcreteType> = types
                    .iter()
                    .filter_map(|vt| match LatticeType::from(vt) {
                        LatticeType::Concrete(ct) => Some(ct),
                        _ => None,
                    })
                    .collect();
                if concrete_types.is_empty() {
                    // Non-empty input whose arms all failed to convert: the
                    // only sound direction is up (unknown), not down.
                    LatticeType::Top
                } else {
                    LatticeType::Union(concrete_types.into_iter().collect())
                }
            }

            // Memory type (no dedicated LatticeType, use Top)
            ValueType::Memory | ValueType::MemoryOf(_) => LatticeType::Top,

            // Dynamic type
            ValueType::Any => LatticeType::Top,
        }
    }
}

/// Convert a VM `ValueType` to a compile-time `LatticeType`, using a struct table
/// to resolve struct names from type IDs.
///
/// This function should be used instead of `LatticeType::from(&ValueType)` when
/// the struct_table is available and accurate struct names are needed for
/// type inference (e.g., when looking up field types in user-defined structs).
///
/// # Arguments
/// * `value_type` - The ValueType to convert
/// * `struct_table` - Map from struct names to StructInfo for name resolution
///
/// # Returns
/// A LatticeType with properly resolved struct names
pub fn value_type_to_lattice_with_struct_table(
    value_type: &ValueType,
    struct_table: &HashMap<String, StructInfo>,
) -> LatticeType {
    match value_type {
        ValueType::ComplexF32 => LatticeType::Concrete(ConcreteType::Struct {
            name: "Complex{Float32}".to_string(),
            type_id: struct_table
                .get("Complex{Float32}")
                .or_else(|| struct_table.get("ComplexF32"))
                .map_or(0, |info| info.type_id),
        }),
        ValueType::ComplexF64 => LatticeType::Concrete(ConcreteType::Struct {
            name: "Complex{Float64}".to_string(),
            type_id: struct_table
                .get("Complex{Float64}")
                .or_else(|| struct_table.get("ComplexF64"))
                .map_or(0, |info| info.type_id),
        }),
        // Struct types - use struct_table to resolve proper name
        ValueType::Struct(type_id) => {
            // Search for struct name by type_id
            for (name, info) in struct_table {
                if info.type_id == *type_id {
                    return LatticeType::Concrete(ConcreteType::Struct {
                        name: name.clone(),
                        type_id: *type_id,
                    });
                }
            }
            // Fallback to synthetic name if not found in table
            LatticeType::Concrete(ConcreteType::Struct {
                name: format!("Struct#{}", type_id),
                type_id: *type_id,
            })
        }
        ValueType::Union(types) => {
            // Empty union is `Union{}` (Bottom) — see the `From<&ValueType>`
            // impl above (Issue #5916).
            if types.is_empty() {
                return LatticeType::Bottom;
            }
            let concrete_types: Vec<ConcreteType> = types
                .iter()
                .filter_map(
                    |vt| match value_type_to_lattice_with_struct_table(vt, struct_table) {
                        LatticeType::Concrete(ct) => Some(ct),
                        _ => None,
                    },
                )
                .collect();
            if concrete_types.is_empty() {
                LatticeType::Top
            } else {
                LatticeType::Union(concrete_types.into_iter().collect())
            }
        }
        // All other types delegate to the standard conversion
        _ => LatticeType::from(value_type),
    }
}

/// Convert a compile-time `LatticeType` to a VM `ValueType`.
///
/// This conversion is used when type inference results need to be
/// translated back into runtime type information for code generation.
impl From<&LatticeType> for ValueType {
    fn from(lattice_type: &LatticeType) -> Self {
        match lattice_type {
            // `Union{}` (Bottom, unreachable code) deliberately widens to
            // `Any` (Issue #5916). `ValueType` drives codegen, whose strict
            // consumers (field access, coercion, ...) must still compile
            // inference-unreachable code, and in-progress recursive-call
            // estimates can surface here as `Bottom` (e.g. `Meta.unblock`).
            // Mapping to the exact empty-union carrier instead was tried and
            // reverted: it turned such leaks into compile errors. This is a
            // sound over-approximation but NOT lattice-faithful — the round
            // trip `Bottom → ValueType → LatticeType` widens to `Top`.
            // Precision-sensitive callers (reflection, `return_types`) must
            // use `lattice_to_parametric_julia_type` / `lattice_to_julia_type`,
            // which preserve `Bottom` (Issue #4679). Note the VM-side
            // spelling of `Union{}` (`ValueType::Union(vec![])`, produced by
            // `julia_type_to_value_type` for `JuliaType::Bottom`) DOES map
            // back to `LatticeType::Bottom`, so that carrier round-trips
            // exactly.
            LatticeType::Bottom => ValueType::Any,
            LatticeType::Top => ValueType::Any, // Unknown type - use Any

            // Convert const to its concrete type for runtime
            LatticeType::Const(cv) => {
                let concrete = cv.to_concrete_type();
                ValueType::from(&LatticeType::Concrete(concrete))
            }

            LatticeType::Concrete(concrete) => match concrete {
                // Issue #6916: every nullary primitive routes through the
                // single-source-of-truth `From<&CorePrimitive> for ValueType`
                // (defined above), collapsing the former 21 per-primitive arms.
                // Disjoint from the abstract / `Any` / catch-all `Core(_)` arms
                // below, so behaviour is unchanged.
                ConcreteType::Core(CoreType::Primitive(p)) => ValueType::from(p),

                // Array types
                ConcreteType::Array { element, .. } => {
                    let elem_type = convert_concrete_to_array_element(element);
                    ValueType::ArrayOf(elem_type, None)
                }

                // Tuple types
                ConcreteType::Tuple { elements } => {
                    // For now, fall back to generic Tuple
                    // Future: could convert to specific tuple representation
                    let _ = elements; // Suppress unused warning
                    ValueType::Tuple
                }

                // Tuple with Vararg tail (Issue #3511): inference-only shape.
                // Codegen / VM still see a generic Tuple value type until
                // the runtime is updated to consume Vararg-tail tuples.
                ConcreteType::TupleVararg { .. } => ValueType::Tuple,

                // NamedTuple types
                ConcreteType::NamedTuple { fields } => {
                    // For now, fall back to generic NamedTuple
                    let _ = fields; // Suppress unused warning
                    ValueType::NamedTuple
                }

                // Struct types
                ConcreteType::Struct { name, type_id } => match name.as_str() {
                    "Complex{Float32}" | "ComplexF32" => ValueType::ComplexF32,
                    "Complex{Float64}" | "ComplexF64" => ValueType::ComplexF64,
                    _ => ValueType::Struct(*type_id),
                },

                // Function/closure types - no direct ValueType equivalent
                ConcreteType::Function { .. }
                | ConcreteType::Closure { .. }
                | ConcreteType::ComposedFunction { .. } => ValueType::Any,

                // Range types
                ConcreteType::Range { .. } => ValueType::Range,

                // Dictionary type
                ConcreteType::Dict { .. } => ValueType::Dict,

                // Set type
                ConcreteType::Set { .. } => ValueType::Set,

                // Generator type
                ConcreteType::Generator { .. } => ValueType::Generator,

                // Pairs type
                ConcreteType::Pairs => ValueType::Pairs,

                // Type system types
                ConcreteType::DataType { .. } => ValueType::DataType,
                ConcreteType::Module { .. } => ValueType::Module,

                // IO type
                ConcreteType::Core(CoreType::Abstract(CoreAbstract::IO)) => ValueType::IO,

                // Metaprogramming types
                ConcreteType::Expr => ValueType::Expr,
                ConcreteType::QuoteNode => ValueType::QuoteNode,
                ConcreteType::LineNumberNode => ValueType::LineNumberNode,
                ConcreteType::GlobalRef => ValueType::GlobalRef,

                // Regex types
                ConcreteType::Regex => ValueType::Regex,
                ConcreteType::RegexMatch => ValueType::RegexMatch,

                // Enum type (Issue #2863)
                ConcreteType::Enum { .. } => ValueType::Enum,

                // Any element type
                ConcreteType::Core(CoreType::Any) => ValueType::Any,

                // Abstract types - no direct runtime representation
                ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number))
                | ConcreteType::Core(CoreType::Abstract(CoreAbstract::Integer))
                | ConcreteType::Core(CoreType::Abstract(CoreAbstract::AbstractFloat)) => {
                    ValueType::Any
                }

                // Union types (element type unions) - convert to ValueType::Union
                ConcreteType::UnionOf(types) => {
                    let value_types: Vec<ValueType> = types
                        .iter()
                        .map(|ct| ValueType::from(&LatticeType::Concrete(ct.clone())))
                        .collect();
                    ValueType::Union(value_types)
                }

                // Core variants not yet folded to dedicated arms (Issue #6720,
                // Slice-2 step-1a) have no direct ValueType — widen to Any.
                ConcreteType::Core(_) => ValueType::Any,
            },

            // Union types - preserve type information for optimization
            LatticeType::Union(types) => {
                let value_types: Vec<ValueType> = types
                    .iter()
                    .map(|ct| ValueType::from(&LatticeType::Concrete(ct.clone())))
                    .collect();
                ValueType::Union(value_types)
            }

            // Conditional types - fallback to Any
            // These are control-flow sensitive and don't have runtime representation
            LatticeType::Conditional { .. } => ValueType::Any,
        }
    }
}

/// Helper function to convert `ArrayElementType` to `ConcreteType`.
///
/// Used when converting VM array types to lattice types.
fn convert_array_element_type(elem_type: &ArrayElementType) -> ConcreteType {
    match elem_type {
        // Floating point
        ArrayElementType::F32 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)),
        ArrayElementType::F64 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),

        // Complex types — represent as Complex{T} struct so that an array of
        // complex values is not confused with an array of real floats
        // (Issue #3540).
        ArrayElementType::ComplexF32 => ConcreteType::Struct {
            name: "Complex{Float32}".to_string(),
            type_id: 0,
        },
        ArrayElementType::ComplexF64 => ConcreteType::Struct {
            name: "Complex{Float64}".to_string(),
            type_id: 0,
        },

        // Signed integers
        ArrayElementType::I8 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)),
        ArrayElementType::I16 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)),
        ArrayElementType::I32 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)),
        ArrayElementType::I64 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        ArrayElementType::I128 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)),

        // Unsigned integers
        ArrayElementType::U8 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)),
        ArrayElementType::U16 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)),
        ArrayElementType::U32 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)),
        ArrayElementType::U64 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)),
        ArrayElementType::U128 => ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128)),

        // Boolean
        ArrayElementType::Bool => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),

        // String types
        ArrayElementType::String => ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
        // SubString{String} shares the runtime representation with String
        // (Issue #3574). For lattice-type purposes, treat it as String so
        // type inference remains stable across split/rsplit results.
        ArrayElementType::SubString => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))
        }
        ArrayElementType::Char => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)),
        ArrayElementType::Symbol => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol)),
        ArrayElementType::Nothing => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing))
        }

        // Struct types
        ArrayElementType::Struct => ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)), // Generic fallback
        ArrayElementType::StructOf(type_id) => ConcreteType::Struct {
            name: format!("Struct#{}", type_id),
            type_id: *type_id,
        },
        ArrayElementType::StructInlineOf(type_id, _field_count) => ConcreteType::Struct {
            name: format!("Struct#{}", type_id),
            type_id: *type_id,
        },

        // Tuple arrays - fallback to generic representation
        ArrayElementType::TupleOf(_fields) => {
            // For simplicity, represent as a generic tuple
            ConcreteType::Tuple { elements: vec![] }
        }

        // Union logical element types use boxed storage at runtime, but the
        // logical eltype is still inferable. Preserve it so
        // `Vector{Union{Nothing,Int64}}[i]` does not degrade to `Any`.
        ArrayElementType::UnionOf(members) => convert_union_array_element_members(members),
        ArrayElementType::Abstract(_) => ConcreteType::Core(CoreType::Any),

        // Any - preserve unknown element type
        ArrayElementType::Any => ConcreteType::Core(CoreType::Any),
    }
}

/// Convert the structured members of an `ArrayElementType::UnionOf` into a
/// `ConcreteType` (Issue #6720).
///
/// The members are already structured `JuliaType`s, so this canonicalizes them
/// directly (flatten / dedup / subtype-absorb / sort / collapse, Issue #5066)
/// instead of rendering a `Union{...}` string and re-parsing it (the former
/// `bridge.rs` string round-trip called out in TYPE_REPRESENTATIONS.md §3.3c).
/// Behaviour is byte-identical to the old round-trip because `from_name_or_struct`
/// delegated `Union{...}` parsing to the same `canonicalize_union`.
fn convert_union_array_element_members(members: &[JuliaType]) -> ConcreteType {
    match crate::types::canonicalize_union(members.to_vec()) {
        JuliaType::Union(types) => ConcreteType::UnionOf(
            types
                .iter()
                .map(julia_type_to_concrete_type_lossy)
                .collect(),
        ),
        JuliaType::Bottom => ConcreteType::Core(CoreType::Any),
        other => julia_type_to_concrete_type_lossy(&other),
    }
}

/// Canonical structured `JuliaType → ConcreteType` conversion (Issue #5916).
///
/// This is the structured, table-free conversion: parametric struct params stay
/// inside the name string and the `type_id` is defaulted to `0` (resolved later
/// by the lattice). It is the single source of truth that the canonical
/// `JuliaType → LatticeType` bridge below builds on; sibling-owned wrappers
/// (`abstract_interp/engine`, `type_stability/analyzer`) should delegate to it
/// rather than re-deriving the mapping (see §3.4 of TYPE_REPRESENTATIONS.md).
pub(crate) fn julia_type_to_concrete_type_lossy(ty: &JuliaType) -> ConcreteType {
    // Issue #6599 Phase 3 (Slice B): route JuliaType -> ConcreteType through the
    // canonical CoreType hub instead of a divergence-prone direct match. The
    // bare-`Array` -> `Array{Any}` special case (#5916) is preserved because
    // `CoreType::from(&JuliaType::Array)` -> `CoreType::Struct{name:"Array",
    // params:[]}` -> `ConcreteType::from` -> `Array{Any}`. Container types that the
    // old `_ => Any` fallthrough dropped (Tuple/Dict/Set/Range/...) now recover
    // their structure; abstract families without a concrete ConcreteType image
    // stay `Any` (see the per-arm audit in
    // `julia_to_concrete_lossy_via_core_pins_structured_containers_issue_6599`).
    ConcreteType::from(&crate::inference_core::type_core::CoreType::from(ty))
}

/// Canonical `JuliaType → LatticeType` conversion (Issue #5916).
///
/// This is the single in-scope source of truth for lifting a user-facing
/// `JuliaType` annotation into the abstract-interpretation lattice. Four
/// parallel implementations historically existed
/// (`type_stability/analyzer`, `abstract_interp/engine`,
/// `expr/infer/expr_tfuncs`, `vm/builtins_reflection`) and **disagreed** on
/// three points; this canonical function resolves all three in favour of the
/// upstream-correct behaviour:
///
/// 1. **Empty `Union{}` → `Bottom`.** Julia: `typeof(Union{}) ==
///    Core.TypeofBottom`, i.e. `Union{}` *is* `Bottom`. (The reflection copy
///    produced `LatticeType::Union(∅)` and the `expr_tfuncs` copy collapsed the
///    whole union to `Top` — both wrong.)
/// 2. **A `Union` containing `Any` widens to `Top`**, and a non-empty union of
///    concretes is kept as `LatticeType::Union`.
/// 3. **Abstract numeric supertypes are preserved** as their `ConcreteType`
///    abstract markers (`Number`/`Integer`/`AbstractFloat`) so callers can
///    still specialize, instead of widening straight to `Top`.
///
/// Struct resolution is parameterized: when a struct resolver (or table) is
/// supplied, a `JuliaType::Struct(name)` resolves to its registered `type_id`,
/// and an *unresolved* name widens to `Top` (the host knows its full struct
/// universe, so an unknown name is an abstract family spelled as
/// `Struct(name)`, not a concrete struct — see the `Struct` arm below).
/// Without a resolver it keeps the structured `ConcreteType::Struct
/// { type_id: 0 }` spelling (the `type_id` is resolved later by the
/// lattice). Element/parameter
/// conversion recurses through this same function (projected to `ConcreteType`
/// by [`julia_type_to_concrete_or_any_with_struct_resolver`]) so element
/// positions get the identical struct-id resolution and union preservation as
/// top-level annotations; for the concrete-mapping types this agrees with the
/// structured [`julia_type_to_concrete_type_lossy`] (pinned by
/// `test_julia_type_to_lattice_agrees_with_concrete_lossy_issue_5916`).
///
/// Sibling-owned wrappers should delegate here (see §3.6 of
/// `docs/vm/TYPE_REPRESENTATIONS.md` for the call-site list).
pub fn julia_type_to_lattice_with_struct_resolver(
    ty: &JuliaType,
    resolve_struct_id: Option<&dyn Fn(&str) -> Option<usize>>,
) -> LatticeType {
    match ty {
        // Concrete primitives, ranges, dict/set, arrays, tuples, and abstract
        // numeric supertypes all have a faithful structured `ConcreteType`
        // image produced by the canonical lossy conversion.
        JuliaType::Int8
        | JuliaType::Int16
        | JuliaType::Int32
        | JuliaType::Int64
        | JuliaType::Int128
        | JuliaType::BigInt
        | JuliaType::UInt8
        | JuliaType::UInt16
        | JuliaType::UInt32
        | JuliaType::UInt64
        | JuliaType::UInt128
        | JuliaType::Float16
        | JuliaType::Float32
        | JuliaType::Float64
        | JuliaType::BigFloat
        | JuliaType::Bool
        | JuliaType::String
        | JuliaType::Char
        | JuliaType::Nothing
        | JuliaType::Missing
        | JuliaType::Symbol
        | JuliaType::Number
        | JuliaType::Integer
        | JuliaType::AbstractFloat
        | JuliaType::Array => LatticeType::Concrete(julia_type_to_concrete_type_lossy(ty)),
        // Array element positions recurse through the resolver-aware
        // projection so a `Vector{MyStruct}` annotation keeps the registered
        // `type_id` (and a `Vector{Union{...}}` keeps the element union).
        JuliaType::VectorOf(element) | JuliaType::MatrixOf(element) => {
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(julia_type_to_concrete_or_any_with_struct_resolver(
                    element,
                    resolve_struct_id,
                )),
                ndims: None,
            })
        }
        // `Real` widens to the `Number` abstract marker (it has no dedicated
        // `ConcreteType` variant); `Signed`/`Unsigned` widen to `Integer`.
        JuliaType::Real => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)))
        }
        JuliaType::Signed | JuliaType::Unsigned => LatticeType::Concrete(ConcreteType::Core(
            CoreType::Abstract(CoreAbstract::Integer),
        )),
        JuliaType::Tuple => LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] }),
        JuliaType::TupleOf(elements) => LatticeType::Concrete(ConcreteType::Tuple {
            elements: elements
                .iter()
                .map(|element| {
                    julia_type_to_concrete_or_any_with_struct_resolver(element, resolve_struct_id)
                })
                .collect(),
        }),
        JuliaType::Dict => LatticeType::Concrete(ConcreteType::Dict {
            key: Box::new(ConcreteType::Core(CoreType::Any)),
            value: Box::new(ConcreteType::Core(CoreType::Any)),
        }),
        JuliaType::Set => LatticeType::Concrete(ConcreteType::Set {
            element: Box::new(ConcreteType::Core(CoreType::Any)),
        }),
        JuliaType::UnitRange | JuliaType::StepRange => LatticeType::Concrete(ConcreteType::Range {
            element: Box::new(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        }),
        // Upstream-correct union handling (see doc comment above).
        JuliaType::Union(types) => {
            let elements: std::collections::BTreeSet<ConcreteType> = types
                .iter()
                .map(|t| julia_type_to_concrete_or_any_with_struct_resolver(t, resolve_struct_id))
                .collect();
            if elements.is_empty() {
                LatticeType::Bottom
            } else if elements.contains(&ConcreteType::Core(CoreType::Any)) {
                LatticeType::Top
            } else {
                LatticeType::Union(elements)
            }
        }
        // User-defined struct: resolve the `type_id` via the resolver when one
        // is available. When a resolver IS supplied but does not know the
        // name, widen to `Top`: the host handed us its full struct universe,
        // so an unresolved name is not a registered concrete struct — it is
        // typically an abstract family spelled as `Struct(name)` (e.g.
        // `AbstractDict`, since `JuliaType` has no dedicated variant for it),
        // and treating it as a concrete struct sends inference down wrong
        // method branches (caught by the `dict_mergewith` fixture: inferring
        // `d::AbstractDict` as a concrete struct made `mergewith`'s return
        // infer as the error branch). Without a resolver, keep the structured
        // `type_id: 0` placeholder (resolved later by the lattice).
        JuliaType::Struct(name) => match resolve_struct_id {
            Some(resolve) => match resolve(name) {
                Some(type_id) => LatticeType::Concrete(ConcreteType::Struct {
                    name: name.clone(),
                    type_id,
                }),
                None => LatticeType::Top,
            },
            None => LatticeType::Concrete(ConcreteType::Struct {
                name: name.clone(),
                type_id: 0,
            }),
        },
        JuliaType::Any => LatticeType::Top,
        // The canonical `Union{}` spelling is the dedicated `Bottom` variant
        // (`types/julia_type/mod.rs`); it must lower to `LatticeType::Bottom`
        // exactly like the non-canonical `Union(vec![])` spelling above, not
        // fall through to `Top` (Issue #6523). Upstream: `typeof(Union{}) ==
        // Core.TypeofBottom` and a `::Union{}` value/argument is unreachable.
        // Note the `LatticeType::Bottom → ValueType` boundary still
        // deliberately widens to `Any` (§3.5 of TYPE_REPRESENTATIONS.md), so
        // VM carriers are unaffected; this only lets the engine/analyzer hosts
        // treat an annotated/recorded `Union{}` as the lattice identity of
        // `join` instead of the absorbing `Top`.
        JuliaType::Bottom => LatticeType::Bottom,
        // Everything else (typevars, abstract families without a dedicated
        // `ConcreteType` marker, metaprogramming nodes) widens to `Top` to
        // preserve dynamic-dispatch compatibility.
        _ => LatticeType::Top,
    }
}

/// [`julia_type_to_lattice_with_struct_resolver`] specialized to a
/// `compile::context::StructInfo` table (the compiler-side struct registry).
pub fn julia_type_to_lattice_with_struct_table(
    ty: &JuliaType,
    struct_table: Option<&HashMap<String, StructInfo>>,
) -> LatticeType {
    match struct_table {
        Some(table) => julia_type_to_lattice_with_struct_resolver(
            ty,
            Some(&|name: &str| table.get(name).map(|info| info.type_id)),
        ),
        None => julia_type_to_lattice_with_struct_resolver(ty, None),
    }
}

/// Table-free [`julia_type_to_lattice_with_struct_resolver`]. A
/// `JuliaType::Struct(name)` keeps the structured `type_id: 0` placeholder
/// (resolved later by the lattice).
pub fn julia_type_to_lattice(ty: &JuliaType) -> LatticeType {
    julia_type_to_lattice_with_struct_resolver(ty, None)
}

/// Projection of the canonical [`julia_type_to_lattice_with_struct_resolver`]
/// onto `ConcreteType`, used for element/parameter positions (array element,
/// tuple element, union member) and by sibling-owned `JuliaType →
/// ConcreteType` wrappers. A `Union` lattice result is preserved structurally
/// as `ConcreteType::UnionOf` (Issue #5595); everything non-concrete widens to
/// `Any`.
pub(crate) fn julia_type_to_concrete_or_any_with_struct_resolver(
    ty: &JuliaType,
    resolve_struct_id: Option<&dyn Fn(&str) -> Option<usize>>,
) -> ConcreteType {
    match julia_type_to_lattice_with_struct_resolver(ty, resolve_struct_id) {
        LatticeType::Concrete(concrete) => concrete,
        LatticeType::Const(value) => value.to_concrete_type(),
        LatticeType::Union(types) => ConcreteType::UnionOf(types.into_iter().collect()),
        _ => ConcreteType::Core(CoreType::Any),
    }
}

/// Helper function to convert `ConcreteType` to `ArrayElementType`.
///
/// Used when converting lattice array types back to VM array types.
fn convert_concrete_to_array_element(concrete: &ConcreteType) -> ArrayElementType {
    match concrete {
        // Floating point
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)) => ArrayElementType::F32,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)) => ArrayElementType::F64,

        // Signed integers
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)) => ArrayElementType::I8,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)) => ArrayElementType::I16,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)) => ArrayElementType::I32,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)) => ArrayElementType::I64,

        // Unsigned integers
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)) => ArrayElementType::U8,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)) => ArrayElementType::U16,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)) => ArrayElementType::U32,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)) => ArrayElementType::U64,

        // Boolean
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)) => ArrayElementType::Bool,

        // String types
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)) => ArrayElementType::String,
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)) => ArrayElementType::Char,

        // Struct types — recognize Complex{T} placeholders (type_id == 0)
        // emitted by the bridge so that round-tripping a complex array
        // preserves the complex element type (Issue #3540). Real
        // user-defined structs (with a non-zero type_id) fall through to
        // `StructOf(type_id)` so dispatch and AoS storage remain correct.
        ConcreteType::Struct { name, type_id } => match (name.as_str(), *type_id) {
            ("Complex{Float32}", 0) => ArrayElementType::ComplexF32,
            ("Complex{Float64}", 0) => ArrayElementType::ComplexF64,
            (_, id) => ArrayElementType::StructOf(id),
        },

        // Any element type
        ConcreteType::Core(CoreType::Any) => ArrayElementType::Any,
        // Complex types without direct ArrayElementType mapping - fallback to Any
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Missing))
        | ConcreteType::Core(CoreType::Primitive(CorePrimitive::Symbol))
        | ConcreteType::Array { .. }
        | ConcreteType::Tuple { .. }
        // Issue #3511: Vararg-tail tuple has no direct array-element mapping.
        | ConcreteType::TupleVararg { .. }
        | ConcreteType::NamedTuple { .. }
        | ConcreteType::Function { .. }
        | ConcreteType::Closure { .. }
        | ConcreteType::ComposedFunction { .. }
        | ConcreteType::Range { .. }
        | ConcreteType::Dict { .. }
        | ConcreteType::Set { .. }
        | ConcreteType::Generator { .. }
        | ConcreteType::Pairs
        | ConcreteType::DataType { .. }
        | ConcreteType::Module { .. }
        | ConcreteType::Core(CoreType::Abstract(CoreAbstract::IO))
        | ConcreteType::Expr
        | ConcreteType::QuoteNode
        | ConcreteType::LineNumberNode
        | ConcreteType::GlobalRef
        | ConcreteType::Regex
        | ConcreteType::RegexMatch
        | ConcreteType::UnionOf(..)
        // Abstract types - no direct array element type
        | ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number))
        | ConcreteType::Core(CoreType::Abstract(CoreAbstract::Integer))
        | ConcreteType::Core(CoreType::Abstract(CoreAbstract::AbstractFloat))
        // Enum types - stored as i64 internally but no dedicated ArrayElementType
        | ConcreteType::Enum { .. } => ArrayElementType::Any,

        // Core variants not yet folded to dedicated arms (Issue #6720,
        // Slice-2 step-1a) have no dedicated array element type.
        ConcreteType::Core(_) => ArrayElementType::Any,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn test_valuetype_to_latticetype_integers() {
        // Signed integers
        assert_eq!(
            LatticeType::from(&ValueType::I8),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)))
        );
        assert_eq!(
            LatticeType::from(&ValueType::I16),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int16
            )))
        );
        assert_eq!(
            LatticeType::from(&ValueType::I32),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int32
            )))
        );
        assert_eq!(
            LatticeType::from(&ValueType::I64),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        // I128 and BigInt now have proper type representations
        assert_eq!(
            LatticeType::from(&ValueType::I128),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int128
            )))
        );
        assert_eq!(
            LatticeType::from(&ValueType::BigInt),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigInt
            )))
        );

        // Unsigned integers
        assert_eq!(
            LatticeType::from(&ValueType::U8),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt8
            )))
        );
        assert_eq!(
            LatticeType::from(&ValueType::U16),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt16
            )))
        );
        assert_eq!(
            LatticeType::from(&ValueType::U32),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt32
            )))
        );
        assert_eq!(
            LatticeType::from(&ValueType::U64),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt64
            )))
        );
        assert_eq!(
            LatticeType::from(&ValueType::U128),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt128
            )))
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_floats() {
        assert_eq!(
            LatticeType::from(&ValueType::F32),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
        assert_eq!(
            LatticeType::from(&ValueType::F64),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
        // BigFloat now has proper type representation
        assert_eq!(
            LatticeType::from(&ValueType::BigFloat),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::BigFloat
            )))
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_bool() {
        assert_eq!(
            LatticeType::from(&ValueType::Bool),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_issue_4270_union_array_element_type_is_preserved() {
        let result = LatticeType::from(&ValueType::ArrayOf(
            ArrayElementType::union_from_body("Nothing, Int64"),
            None,
        ));

        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::UnionOf(vec![
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing)),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ])),
                ndims: None
            })
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_strings() {
        assert_eq!(
            LatticeType::from(&ValueType::Str),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            )))
        );
        assert_eq!(
            LatticeType::from(&ValueType::Char),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_special() {
        assert_eq!(
            LatticeType::from(&ValueType::Nothing),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Nothing
            )))
        );
        assert_eq!(
            LatticeType::from(&ValueType::Missing),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Missing
            )))
        );
        assert_eq!(
            LatticeType::from(&ValueType::Symbol),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Symbol
            )))
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_arrays() {
        // Legacy array type
        assert_eq!(
            LatticeType::from(&ValueType::Array),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
                ndims: None
            })
        );

        // Typed array
        assert_eq!(
            LatticeType::from(&ValueType::ArrayOf(ArrayElementType::I64, None)),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                ndims: None
            })
        );
        assert_eq!(
            LatticeType::from(&ValueType::ArrayOf(ArrayElementType::F32, None)),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float32
                ))),
                ndims: None
            })
        );
        assert_eq!(
            LatticeType::from(&ValueType::ArrayOf(ArrayElementType::Any, None)),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
                ndims: None
            })
        );
    }

    #[test]
    fn test_valuetype_to_latticetype_complex_arrays() {
        // Issue #3540: complex array element types must not collapse to real
        // float element types.
        let f64_array = LatticeType::from(&ValueType::ArrayOf(ArrayElementType::ComplexF64, None));
        match f64_array {
            LatticeType::Concrete(ConcreteType::Array { element, .. }) => match *element {
                ConcreteType::Struct { name, .. } => assert_eq!(name, "Complex{Float64}"),
                other => panic!(
                    "ComplexF64 array should map to Complex{{Float64}} struct element, got {:?}",
                    other
                ),
            },
            other => panic!("Expected Array, got {:?}", other),
        }

        let f32_array = LatticeType::from(&ValueType::ArrayOf(ArrayElementType::ComplexF32, None));
        match f32_array {
            LatticeType::Concrete(ConcreteType::Array { element, .. }) => match *element {
                ConcreteType::Struct { name, .. } => assert_eq!(name, "Complex{Float32}"),
                other => panic!(
                    "ComplexF32 array should map to Complex{{Float32}} struct element, got {:?}",
                    other
                ),
            },
            other => panic!("Expected Array, got {:?}", other),
        }

        // Round-trip: Complex{T} struct (with type_id == 0) -> ComplexT array element.
        let elem64 = convert_concrete_to_array_element(&ConcreteType::Struct {
            name: "Complex{Float64}".to_string(),
            type_id: 0,
        });
        assert_eq!(elem64, ArrayElementType::ComplexF64);
        let elem32 = convert_concrete_to_array_element(&ConcreteType::Struct {
            name: "Complex{Float32}".to_string(),
            type_id: 0,
        });
        assert_eq!(elem32, ArrayElementType::ComplexF32);

        // A real user-defined struct named "Complex{Float64}" with a non-zero
        // type_id maps to StructOf(type_id), not ComplexF64.
        let user_struct = convert_concrete_to_array_element(&ConcreteType::Struct {
            name: "Complex{Float64}".to_string(),
            type_id: 7,
        });
        assert_eq!(user_struct, ArrayElementType::StructOf(7));
    }

    #[test]
    fn test_valuetype_to_latticetype_struct() {
        let type_id = 42;
        let result = LatticeType::from(&ValueType::Struct(type_id));
        assert!(
            matches!(&result, LatticeType::Concrete(ConcreteType::Struct { .. })),
            "Expected Struct type, got {:?}",
            result
        );
        if let LatticeType::Concrete(ConcreteType::Struct { type_id: id, .. }) = result {
            assert_eq!(id, type_id);
        }
    }

    #[test]
    fn test_valuetype_union_to_lattice_resolves_structs_with_table_issue_4270() {
        let mut struct_table = HashMap::new();
        struct_table.insert(
            "A4270".to_string(),
            StructInfo {
                type_id: 10,
                is_mutable: false,
                fields: vec![],
                has_inner_constructor: false,
            },
        );
        struct_table.insert(
            "B4270".to_string(),
            StructInfo {
                type_id: 11,
                is_mutable: false,
                fields: vec![],
                has_inner_constructor: false,
            },
        );

        let result = value_type_to_lattice_with_struct_table(
            &ValueType::Union(vec![ValueType::Struct(10), ValueType::Struct(11)]),
            &struct_table,
        );

        let LatticeType::Union(types) = result else {
            panic!("expected union lattice type");
        };
        assert!(types.contains(&ConcreteType::Struct {
            name: "A4270".to_string(),
            type_id: 10,
        }));
        assert!(types.contains(&ConcreteType::Struct {
            name: "B4270".to_string(),
            type_id: 11,
        }));
    }

    #[test]
    fn test_valuetype_to_latticetype_top() {
        assert_eq!(LatticeType::from(&ValueType::Any), LatticeType::Top);
        // Range, Dict, Set now have concrete type representations
        assert!(matches!(
            LatticeType::from(&ValueType::Range),
            LatticeType::Concrete(ConcreteType::Range { .. })
        ));
        assert!(matches!(
            LatticeType::from(&ValueType::Dict),
            LatticeType::Concrete(ConcreteType::Dict { .. })
        ));
        assert!(matches!(
            LatticeType::from(&ValueType::Set),
            LatticeType::Concrete(ConcreteType::Set { .. })
        ));
    }

    #[test]
    fn test_latticetype_to_valuetype_integers() {
        // Signed integers
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int8)
            ))),
            ValueType::I8
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int16)
            ))),
            ValueType::I16
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int32)
            ))),
            ValueType::I32
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            ))),
            ValueType::I64
        );

        // Unsigned integers
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::UInt8)
            ))),
            ValueType::U8
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::UInt16)
            ))),
            ValueType::U16
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::UInt32)
            ))),
            ValueType::U32
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::UInt64)
            ))),
            ValueType::U64
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_floats() {
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float32)
            ))),
            ValueType::F32
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64)
            ))),
            ValueType::F64
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_bool() {
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Bool)
            ))),
            ValueType::Bool
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_special() {
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Nothing)
            ))),
            ValueType::Nothing
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Missing)
            ))),
            ValueType::Missing
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Symbol)
            ))),
            ValueType::Symbol
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Core(CoreType::Any))),
            ValueType::Any
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_arrays() {
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                ndims: None
            })),
            ValueType::ArrayOf(ArrayElementType::I64, None)
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float32
                ))),
                ndims: None
            })),
            ValueType::ArrayOf(ArrayElementType::F32, None)
        );
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
                ndims: None
            })),
            ValueType::ArrayOf(ArrayElementType::Any, None)
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_struct() {
        let type_id = 42;
        assert_eq!(
            ValueType::from(&LatticeType::Concrete(ConcreteType::Struct {
                name: "Test".to_string(),
                type_id
            })),
            ValueType::Struct(type_id)
        );
    }

    #[test]
    fn test_latticetype_to_valuetype_top_bottom() {
        assert_eq!(ValueType::from(&LatticeType::Top), ValueType::Any);
        // Bottom deliberately widens to Any in the codegen direction — see
        // the `From` impl comment (Issue #5916): the exact empty-union
        // carrier was tried and reverted because in-progress recursive-call
        // Bottom estimates surface at call sites and strict codegen
        // consumers then reject inference-unreachable code (`Meta.unblock`).
        assert_eq!(ValueType::from(&LatticeType::Bottom), ValueType::Any);
    }

    /// Issue #5916: `Union{}` spelled on the VM side (the empty-union carrier
    /// produced by `julia_type_to_value_type(JuliaType::Bottom)`) must map to
    /// `LatticeType::Bottom`, not invert the lattice to `Top`.
    #[test]
    fn test_empty_union_value_type_is_bottom_issue_5916() {
        let empty_union = ValueType::Union(Vec::new());
        assert_eq!(LatticeType::from(&empty_union), LatticeType::Bottom);

        // The table-aware variant agrees with the `From` impl.
        let struct_table: HashMap<String, StructInfo> = HashMap::new();
        assert_eq!(
            value_type_to_lattice_with_struct_table(&empty_union, &struct_table),
            LatticeType::Bottom
        );

        // Top stays a fixed point: Top → Any → Top.
        assert_eq!(
            LatticeType::from(&ValueType::from(&LatticeType::Top)),
            LatticeType::Top
        );

        // Documented residual widening (NOT an identity): the codegen
        // direction maps Bottom to Any, so the lattice-side round trip
        // over-approximates to Top. Pin it so any future change is deliberate.
        assert_eq!(
            LatticeType::from(&ValueType::from(&LatticeType::Bottom)),
            LatticeType::Top
        );
    }

    /// Issue #5916: lattice laws must survive the conversion boundary —
    /// the image of Bottom must still be the identity of `join` and the
    /// absorbing element of `meet`.
    #[test]
    fn test_join_meet_laws_at_conversion_boundary_issue_5916() {
        let bottom_image = LatticeType::from(&ValueType::Union(Vec::new()));
        let int = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));

        assert_eq!(bottom_image.join(&int), int);
        assert_eq!(int.join(&bottom_image), int);
        assert_eq!(bottom_image.meet(&int), LatticeType::Bottom);
        assert_eq!(int.meet(&bottom_image), LatticeType::Bottom);

        // A non-empty union whose arms convert keeps the old behavior.
        let union_vt = ValueType::Union(vec![ValueType::I64, ValueType::F64]);
        assert!(matches!(
            LatticeType::from(&union_vt),
            LatticeType::Union(_)
        ));
    }

    /// Issue #5916: the total `LatticeType → JuliaType` conversion preserves
    /// `Union{}` (`JuliaType` has a Bottom variant, unlike `ValueType`).
    #[test]
    fn test_lattice_to_julia_type_preserves_bottom_issue_5916() {
        assert_eq!(
            lattice_to_julia_type(&LatticeType::Bottom),
            JuliaType::Bottom
        );
        // The partial parametric variant agrees (Issue #4679 arm).
        assert_eq!(
            lattice_to_parametric_julia_type(&LatticeType::Bottom),
            Some(JuliaType::Bottom)
        );
        // Top still widens to Any in the total conversion.
        assert_eq!(lattice_to_julia_type(&LatticeType::Top), JuliaType::Any);
    }

    #[test]
    fn test_latticetype_to_valuetype_union() {
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )));
        types.insert(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )));
        // Union types are now preserved instead of collapsing to Any
        let result = ValueType::from(&LatticeType::Union(types));
        assert!(
            matches!(&result, ValueType::Union(_)),
            "Expected ValueType::Union, got {:?}",
            result
        );
        if let ValueType::Union(value_types) = result {
            assert_eq!(value_types.len(), 2);
            assert!(value_types.contains(&ValueType::I64));
            assert!(value_types.contains(&ValueType::F64));
        }
    }

    #[test]
    fn test_latticetype_to_valuetype_conditional() {
        let conditional = LatticeType::Conditional {
            slot: "x".to_string(),
            then_type: Box::new(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64),
            ))),
            else_type: Box::new(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64),
            ))),
        };
        assert_eq!(ValueType::from(&conditional), ValueType::Any);
    }

    #[test]
    fn test_round_trip_basic_types() {
        // Test round-trip conversions for basic types
        let value_types = vec![
            ValueType::I8,
            ValueType::I16,
            ValueType::I32,
            ValueType::I64,
            ValueType::U8,
            ValueType::U16,
            ValueType::U32,
            ValueType::U64,
            ValueType::Bool,
            ValueType::F32,
            ValueType::F64,
            ValueType::Str,
            ValueType::Char,
            ValueType::Nothing,
            ValueType::Missing,
            ValueType::Symbol,
        ];

        for vt in value_types {
            let lattice = LatticeType::from(&vt);
            let back = ValueType::from(&lattice);
            assert_eq!(vt, back, "Round-trip failed for {:?}", vt);
        }
    }

    #[test]
    fn test_round_trip_array_types() {
        let value_types = vec![
            ValueType::ArrayOf(ArrayElementType::I64, None),
            ValueType::ArrayOf(ArrayElementType::F32, None),
            ValueType::ArrayOf(ArrayElementType::F64, None),
            ValueType::ArrayOf(ArrayElementType::Bool, None),
            ValueType::ArrayOf(ArrayElementType::U8, None),
        ];

        for vt in value_types {
            let lattice = LatticeType::from(&vt);
            let back = ValueType::from(&lattice);
            assert_eq!(vt, back, "Round-trip failed for {:?}", vt);
        }
    }

    #[test]
    fn test_round_trip_struct_type() {
        let type_id = 42;
        let vt = ValueType::Struct(type_id);
        let lattice = LatticeType::from(&vt);
        let back = ValueType::from(&lattice);
        assert_eq!(vt, back, "Round-trip failed for Struct");
    }

    /// Test that ValueType::Enum converts to ConcreteType::Enum (Issue #2863).
    /// Enum types were previously mapped to LatticeType::Top, which lost type information.
    #[test]
    fn test_enum_type_maps_to_concrete_enum_not_top() {
        let vt = ValueType::Enum;
        let lattice = LatticeType::from(&vt);

        // Must be Concrete(Enum), NOT Top (which was the old workaround)
        assert!(
            matches!(lattice, LatticeType::Concrete(ConcreteType::Enum { .. })),
            "Expected Concrete(Enum), got {:?}",
            lattice
        );
    }

    #[test]
    fn test_enum_round_trip() {
        let vt = ValueType::Enum;
        let lattice = LatticeType::from(&vt);
        let back = ValueType::from(&lattice);
        assert_eq!(vt, back, "Round-trip failed for Enum");
    }

    /// Test that ALL ValueType variants can be converted to LatticeType without panicking.
    /// This test ensures exhaustive coverage when new variants are added to ValueType.
    /// If this test fails to compile, it means a new ValueType variant was added
    /// but not handled in the conversion functions.
    #[test]
    fn test_all_valuetype_variants_to_lattice() {
        use crate::vm::value::ArrayElementType;

        // List ALL ValueType variants - if a new variant is added, this test
        // will fail to compile until the new variant is added here.
        let all_variants: Vec<ValueType> = vec![
            // Signed integers
            ValueType::I8,
            ValueType::I16,
            ValueType::I32,
            ValueType::I64,
            ValueType::I128,
            ValueType::BigInt,
            // Unsigned integers
            ValueType::U8,
            ValueType::U16,
            ValueType::U32,
            ValueType::U64,
            ValueType::U128,
            // Boolean
            ValueType::Bool,
            // Floating point
            ValueType::F16,
            ValueType::F32,
            ValueType::F64,
            ValueType::BigFloat,
            // Collections
            ValueType::Array,
            ValueType::ArrayOf(ArrayElementType::F64, None),
            ValueType::Range,
            // String types
            ValueType::Str,
            ValueType::Char,
            // Special types
            ValueType::Nothing,
            ValueType::Missing,
            ValueType::Struct(0),
            ValueType::Rng,
            ValueType::Tuple,
            ValueType::NamedTuple,
            ValueType::Pairs,
            ValueType::Dict,
            ValueType::Set,
            ValueType::Generator,
            ValueType::DataType,
            ValueType::Module,
            ValueType::Function,
            ValueType::IO,
            // Macro system types
            ValueType::Symbol,
            ValueType::Expr,
            ValueType::QuoteNode,
            ValueType::LineNumberNode,
            ValueType::GlobalRef,
            // Regex types
            ValueType::Regex,
            ValueType::RegexMatch,
            // Dynamic type
            ValueType::Any,
            // Union type
            ValueType::Union(vec![ValueType::I64, ValueType::F64]),
            // Enum type
            ValueType::Enum,
        ];

        // Verify each variant can be converted to LatticeType without panicking
        for vt in &all_variants {
            let _lattice = LatticeType::from(vt);
            // If this doesn't panic, the variant is properly handled
        }

        // Also verify we can convert back from LatticeType to ValueType
        for vt in &all_variants {
            let lattice = LatticeType::from(vt);
            let _back = ValueType::from(&lattice);
            // If this doesn't panic, the variant is properly handled
        }
    }

    // ── Canonical JuliaType → LatticeType (Issue #5916, wave 3) ──────────────

    #[test]
    fn test_julia_type_to_lattice_primitives_issue_5916() {
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Int64),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Float64),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Bool),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
        assert_eq!(
            julia_type_to_lattice(&JuliaType::String),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String
            )))
        );
        assert_eq!(julia_type_to_lattice(&JuliaType::Any), LatticeType::Top);
    }

    #[test]
    fn test_julia_type_to_lattice_abstract_numerics_preserved_issue_5916() {
        // Abstract numeric supertypes are preserved as their abstract marker so
        // callers can still specialize (resolves the disagreement where some
        // copies widened these straight to Top).
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Number),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)))
        );
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Real),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(CoreAbstract::Number)))
        );
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Integer),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Signed),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::Integer
            )))
        );
        assert_eq!(
            julia_type_to_lattice(&JuliaType::AbstractFloat),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Abstract(
                CoreAbstract::AbstractFloat
            )))
        );
    }

    #[test]
    fn test_julia_type_to_lattice_empty_union_is_bottom_issue_5916() {
        // Julia: `typeof(Union{}) == Core.TypeofBottom`, i.e. `Union{}` IS
        // `Bottom`. The reflection copy produced `Union(∅)` and the
        // `expr_tfuncs` copy collapsed to `Top`; the canonical impl maps to
        // `Bottom`.
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Union(vec![])),
            LatticeType::Bottom
        );
    }

    /// Issue #6523: the CANONICAL `Union{}` spelling is the dedicated
    /// `JuliaType::Bottom` variant (`types/julia_type/mod.rs`); it must agree
    /// with the empty-union spelling above and lower to `Bottom`, not fall
    /// through the `_ => Top` arm (the widest type instead of the narrowest).
    #[test]
    fn test_julia_type_to_lattice_bottom_variant_is_bottom_issue_6523() {
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Bottom),
            LatticeType::Bottom
        );
        // Both spellings of `Union{}` agree.
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Bottom),
            julia_type_to_lattice(&JuliaType::Union(vec![]))
        );
        // The element/parameter projection still widens Bottom to `Any`
        // (`ConcreteType` has no Bottom variant), matching the §3.5 carrier
        // policy.
        assert_eq!(
            julia_type_to_concrete_or_any_with_struct_resolver(&JuliaType::Bottom, None),
            ConcreteType::Core(CoreType::Any)
        );
        // Round-trip: `lattice_to_julia_type(julia_type_to_lattice(Bottom))`
        // is now the identity (both directions preserve Bottom).
        assert_eq!(
            lattice_to_julia_type(&julia_type_to_lattice(&JuliaType::Bottom)),
            JuliaType::Bottom
        );
    }

    #[test]
    fn test_julia_type_to_lattice_union_rules_issue_5916() {
        // Concrete union is preserved structurally.
        let union = JuliaType::Union(vec![JuliaType::Int64, JuliaType::Float64]);
        match julia_type_to_lattice(&union) {
            LatticeType::Union(elements) => {
                assert!(elements.contains(&ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))));
                assert!(elements.contains(&ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))));
            }
            other => panic!("expected Union, got {other:?}"),
        }
        // A union that contains `Any` widens to `Top`.
        let with_any = JuliaType::Union(vec![JuliaType::Int64, JuliaType::Any]);
        assert_eq!(julia_type_to_lattice(&with_any), LatticeType::Top);
    }

    #[test]
    fn test_julia_type_to_lattice_struct_table_resolution_issue_5916() {
        // Without a table, a user struct keeps the structured placeholder
        // (`type_id: 0`) rather than widening to `Top`.
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Struct("Foo".to_string())),
            LatticeType::Concrete(ConcreteType::Struct {
                name: "Foo".to_string(),
                type_id: 0,
            })
        );
        // With a table, the registered `type_id` is resolved.
        let mut table: HashMap<String, StructInfo> = HashMap::new();
        table.insert(
            "Foo".to_string(),
            StructInfo {
                type_id: 42,
                is_mutable: false,
                fields: vec![],
                has_inner_constructor: false,
            },
        );
        assert_eq!(
            julia_type_to_lattice_with_struct_table(
                &JuliaType::Struct("Foo".to_string()),
                Some(&table)
            ),
            LatticeType::Concrete(ConcreteType::Struct {
                name: "Foo".to_string(),
                type_id: 42,
            })
        );
        // With a table, a name the table does NOT know widens to `Top`: the
        // host supplied its full struct universe, so an unresolved name is an
        // abstract family spelled as `Struct(name)` (e.g. `AbstractDict`),
        // not a concrete struct (regression: `dict_mergewith` fixture).
        assert_eq!(
            julia_type_to_lattice_with_struct_table(
                &JuliaType::Struct("AbstractDict".to_string()),
                Some(&table)
            ),
            LatticeType::Top
        );
    }

    #[test]
    fn test_julia_type_to_lattice_bare_array_is_array_any_issue_5916() {
        // Bare `Array` annotation: structurally an array of unknown element
        // type. The lossy `_` fallback used to collapse it to `Any`, drifting
        // from every historical sibling lattice copy (`Array{Any}`).
        let expected = ConcreteType::Array {
            element: Box::new(ConcreteType::Core(CoreType::Any)),
            ndims: None,
        };
        assert_eq!(
            julia_type_to_lattice(&JuliaType::Array),
            LatticeType::Concrete(expected.clone())
        );
        assert_eq!(
            julia_type_to_concrete_type_lossy(&JuliaType::Array),
            expected
        );
    }

    #[test]
    fn test_julia_type_to_lattice_element_struct_id_resolved_issue_5916() {
        // Element positions recurse through the resolver-aware projection:
        // `Vector{Foo}` keeps Foo's registered `type_id` (the engine's
        // historical behavior, preserved by the wave-4 delegation), and the
        // element union of `Vector{Union{Int64,Nothing}}` is preserved.
        let mut table: HashMap<String, StructInfo> = HashMap::new();
        table.insert(
            "Foo".to_string(),
            StructInfo {
                type_id: 7,
                is_mutable: false,
                fields: vec![],
                has_inner_constructor: false,
            },
        );
        let vec_of_foo = JuliaType::VectorOf(Box::new(JuliaType::Struct("Foo".to_string())));
        assert_eq!(
            julia_type_to_lattice_with_struct_table(&vec_of_foo, Some(&table)),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Struct {
                    name: "Foo".to_string(),
                    type_id: 7,
                }),
                ndims: None
            })
        );
        let vec_of_union = JuliaType::VectorOf(Box::new(JuliaType::Union(vec![
            JuliaType::Int64,
            JuliaType::Nothing,
        ])));
        let LatticeType::Concrete(ConcreteType::Array { element, .. }) =
            julia_type_to_lattice(&vec_of_union)
        else {
            panic!("expected array lattice type");
        };
        let ConcreteType::UnionOf(members) = *element else {
            panic!("expected union element type");
        };
        assert_eq!(
            members
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing))
            ]
            .into_iter()
            .collect()
        );
    }

    #[test]
    fn test_julia_type_to_lattice_agrees_with_concrete_lossy_issue_5916() {
        // The lattice bridge and the structured ConcreteType bridge must never
        // drift: every concrete-mapping JuliaType produces the same
        // ConcreteType through both paths.
        for ty in [
            JuliaType::Int8,
            JuliaType::UInt64,
            JuliaType::Float32,
            JuliaType::Bool,
            JuliaType::String,
            JuliaType::Char,
            JuliaType::Symbol,
            JuliaType::Number,
            JuliaType::Integer,
            JuliaType::AbstractFloat,
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
        ] {
            if let LatticeType::Concrete(ct) = julia_type_to_lattice(&ty) {
                assert_eq!(
                    ct,
                    julia_type_to_concrete_type_lossy(&ty),
                    "drift for {ty:?}"
                );
            } else {
                panic!("expected Concrete for {ty:?}");
            }
        }
    }

    /// Issue #6599: unify the divergent `LatticeType → JuliaType` pair.
    ///
    /// The partial structure-preserving converter
    /// (`lattice_to_parametric_julia_type`, table row #14) and the total
    /// converter (`lattice_to_julia_type`, row #15, via
    /// `concrete_type_to_julia_type`) historically disagreed on a braced
    /// `ConcreteType::Struct { name }`: the partial one parsed the spelling
    /// through `from_name_or_struct` (recovering `Vector{Int64}` →
    /// `JuliaType::VectorOf`, `Complex{Float64}` → its canonical struct form),
    /// while the total one kept the opaque `JuliaType::Struct(name)` string.
    /// They now agree: the total path also parses braced parametric struct
    /// spellings, so reflection callers see the same JuliaType regardless of
    /// which converter ran (verified against upstream `julia` 1.12:
    /// `Vector{Int64}` is a concrete parametric `DataType`, not an opaque
    /// struct name).
    #[test]
    fn test_lattice_to_julia_type_pair_agrees_on_braced_struct_issue_6599() {
        // Braced parametric struct spellings are now parsed identically by the
        // partial (#14) and total (#15) `LatticeType → JuliaType` converters.
        for name in [
            "Vector{Int64}",
            "Matrix{Float64}",
            "Complex{Float64}",
            // A genuine user struct spelling round-trips to itself either way.
            "Point{Int64}",
        ] {
            let lt = LatticeType::Concrete(ConcreteType::Struct {
                name: name.to_string(),
                type_id: 0,
            });
            let total = lattice_to_julia_type(&lt);
            let expected = JuliaType::from_name_or_struct(name);
            assert_eq!(
                total, expected,
                "lattice_to_julia_type must parse braced struct spelling {name:?}"
            );

            // The partial converter has a dedicated braced arm; the two must
            // produce the same JuliaType so the pair is unified (no
            // structure-preserving vs string-opaque divergence).
            assert_eq!(
                lattice_to_parametric_julia_type(&lt),
                Some(total.clone()),
                "partial and total LatticeType→JuliaType must agree for {name:?}"
            );
        }
    }

    /// Issue #6599: the unification must not perturb the bare (non-braced)
    /// struct spelling, the numeric primitives, or the deliberately preserved
    /// `Bottom`/`Top` edges.
    #[test]
    fn test_lattice_to_julia_type_unification_preserves_pins_issue_6599() {
        // Bare struct name stays an opaque struct (no spurious parsing).
        assert_eq!(
            concrete_type_to_julia_type(&ConcreteType::Struct {
                name: "Foo".to_string(),
                type_id: 7,
            }),
            JuliaType::Struct("Foo".to_string())
        );
        // Bare alias names are NOT reinterpreted — only braced spellings are
        // parsed, so a struct literally named `ComplexF32` keeps its opaque
        // form (conservative gate, Issue #6599).
        assert_eq!(
            concrete_type_to_julia_type(&ConcreteType::Struct {
                name: "ComplexF32".to_string(),
                type_id: 0,
            }),
            JuliaType::Struct("ComplexF32".to_string())
        );
        // Numeric primitive unchanged.
        assert_eq!(
            concrete_type_to_julia_type(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))),
            JuliaType::Int64
        );
        // Deliberate Bottom/Top edges (§3.5) unchanged.
        assert_eq!(
            lattice_to_julia_type(&LatticeType::Bottom),
            JuliaType::Bottom
        );
        assert_eq!(lattice_to_julia_type(&LatticeType::Top), JuliaType::Any);
    }

    /// Issue #6916 (epic #5916): `concrete_type_to_julia_type` routes every
    /// nullary primitive through the shared `core_type_to_julia_type` hub
    /// instead of maintaining a duplicate 21-entry primitive table. This pins:
    /// (a) each primitive maps to the expected `JuliaType`; (b) the
    /// concrete-side result is byte-identical to the `CoreType` hub — the
    /// invariant the dedup relies on; and (c) abstract / `Any` cases still widen
    /// to `JuliaType::Any` via the catch-all. The delegation is deliberately
    /// restricted to primitives: the hub maps abstracts to their *own*
    /// `JuliaType` (`Number`/`Integer`/…), so routing them through it would be a
    /// behaviour change — this test guards against that.
    #[test]
    fn concrete_type_to_julia_type_routes_primitives_through_core_hub_issue_6916() {
        use crate::inference_core::core_type_to_julia_type;

        let primitives = [
            (CorePrimitive::Int8, JuliaType::Int8),
            (CorePrimitive::Int16, JuliaType::Int16),
            (CorePrimitive::Int32, JuliaType::Int32),
            (CorePrimitive::Int64, JuliaType::Int64),
            (CorePrimitive::Int128, JuliaType::Int128),
            (CorePrimitive::BigInt, JuliaType::BigInt),
            (CorePrimitive::UInt8, JuliaType::UInt8),
            (CorePrimitive::UInt16, JuliaType::UInt16),
            (CorePrimitive::UInt32, JuliaType::UInt32),
            (CorePrimitive::UInt64, JuliaType::UInt64),
            (CorePrimitive::UInt128, JuliaType::UInt128),
            (CorePrimitive::Bool, JuliaType::Bool),
            (CorePrimitive::Float16, JuliaType::Float16),
            (CorePrimitive::Float32, JuliaType::Float32),
            (CorePrimitive::Float64, JuliaType::Float64),
            (CorePrimitive::BigFloat, JuliaType::BigFloat),
            (CorePrimitive::String, JuliaType::String),
            (CorePrimitive::Char, JuliaType::Char),
            (CorePrimitive::Symbol, JuliaType::Symbol),
            (CorePrimitive::Nothing, JuliaType::Nothing),
            (CorePrimitive::Missing, JuliaType::Missing),
        ];
        for (p, expected) in primitives {
            let core = CoreType::Primitive(p.clone());
            let ct = ConcreteType::Core(core.clone());
            // (a) pinned mapping.
            assert_eq!(
                concrete_type_to_julia_type(&ct),
                expected,
                "primitive {p:?} must map to {expected:?}"
            );
            // (b) byte-identical to the `CoreType` hub (the dedup invariant).
            assert_eq!(
                concrete_type_to_julia_type(&ct),
                core_type_to_julia_type(&core),
                "concrete primitive {p:?} must equal the CoreType hub result"
            );
        }

        // (c) abstracts and `Any` still widen to `Any` via the catch-all — the
        // delegation must NOT reach them.
        for abstract_ty in [
            CoreAbstract::Number,
            CoreAbstract::Integer,
            CoreAbstract::AbstractFloat,
            CoreAbstract::IO,
        ] {
            assert_eq!(
                concrete_type_to_julia_type(&ConcreteType::Core(CoreType::Abstract(abstract_ty))),
                JuliaType::Any
            );
        }
        assert_eq!(
            concrete_type_to_julia_type(&ConcreteType::Core(CoreType::Any)),
            JuliaType::Any
        );
    }

    /// Issue #6916 (epic #5916): `impl From<&CorePrimitive> for ValueType` is
    /// the single source of truth for the nullary-primitive → `ValueType`
    /// mapping. This pins (a) each `CorePrimitive` maps to the expected
    /// `ValueType`, and (b) the `LatticeType → ValueType` reverse bridge routes
    /// every `Core(Primitive(_))` through it (so the bridge's per-primitive
    /// arms can collapse to a single delegating arm without changing
    /// behaviour). The mapping is a bijection with `From<&CorePrimitive>` being
    /// exhaustive over `CorePrimitive`, so a new primitive is a compile error
    /// here rather than a silent miss.
    #[test]
    fn value_type_from_core_primitive_is_reverse_bridge_source_of_truth_issue_6916() {
        let cases = [
            (CorePrimitive::Int8, ValueType::I8),
            (CorePrimitive::Int16, ValueType::I16),
            (CorePrimitive::Int32, ValueType::I32),
            (CorePrimitive::Int64, ValueType::I64),
            (CorePrimitive::Int128, ValueType::I128),
            (CorePrimitive::BigInt, ValueType::BigInt),
            (CorePrimitive::UInt8, ValueType::U8),
            (CorePrimitive::UInt16, ValueType::U16),
            (CorePrimitive::UInt32, ValueType::U32),
            (CorePrimitive::UInt64, ValueType::U64),
            (CorePrimitive::UInt128, ValueType::U128),
            (CorePrimitive::Bool, ValueType::Bool),
            (CorePrimitive::Float16, ValueType::F16),
            (CorePrimitive::Float32, ValueType::F32),
            (CorePrimitive::Float64, ValueType::F64),
            (CorePrimitive::BigFloat, ValueType::BigFloat),
            (CorePrimitive::String, ValueType::Str),
            (CorePrimitive::Char, ValueType::Char),
            (CorePrimitive::Nothing, ValueType::Nothing),
            (CorePrimitive::Missing, ValueType::Missing),
            (CorePrimitive::Symbol, ValueType::Symbol),
        ];
        for (p, expected) in cases {
            // (a) the source-of-truth conversion.
            assert_eq!(
                ValueType::from(&p),
                expected,
                "CorePrimitive {p:?} must map to {expected:?}"
            );
            // (b) the reverse bridge routes the matching lattice type identically.
            let lt = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(p.clone())));
            assert_eq!(
                ValueType::from(&lt),
                expected,
                "reverse bridge must map Core(Primitive({p:?})) to {expected:?}"
            );
        }
    }

    /// Issue #6599 Phase 3 (Slice B) regression pin.
    ///
    /// After rerouting `julia_type_to_concrete_type_lossy` through the canonical
    /// `CoreType` hub (`ConcreteType::from(&CoreType::from(ty))`), this pins the
    /// FINAL `ConcreteType` image for a comprehensive `JuliaType` corpus.
    ///
    /// The reroute was a precision improvement: container families the old
    /// `_ => Any` fallthrough dropped (`TupleOf`/`Tuple`/`Dict`/`Set`/
    /// `NamedTuple`/`UnitRange`/`StepRange`/`Function`) now recover structure;
    /// the per-change diff (old `Any` → new structured) was characterized before
    /// the reroute and is recorded in the PR. Abstract families without a
    /// concrete `ConcreteType` image (`Real`/`Signed`/`Unsigned`/`AbstractRange`)
    /// stay `Any`. Primitives, `Union`, `VectorOf`, bare `Array` (→ `Array{Any}`,
    /// #5916) and user `Struct(name)` are unchanged.
    #[test]
    fn julia_to_concrete_lossy_via_core_pins_structured_containers_issue_6599() {
        let cases: Vec<(JuliaType, ConcreteType)> = vec![
            // Primitives — unchanged.
            (
                JuliaType::Int64,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            ),
            (
                JuliaType::Float64,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ),
            (
                JuliaType::Bool,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)),
            ),
            (
                JuliaType::String,
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::String)),
            ),
            // bare `Array` keeps the `Array{Any}` special case (#5916).
            (
                JuliaType::Array,
                ConcreteType::Array {
                    element: Box::new(ConcreteType::Core(CoreType::Any)),
                    ndims: None,
                },
            ),
            // `VectorOf` — unchanged.
            (
                JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                ConcreteType::Array {
                    element: Box::new(ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::Int64,
                    ))),
                    ndims: None,
                },
            ),
            // `Union` — unchanged.
            (
                JuliaType::Union(vec![JuliaType::Int64, JuliaType::Float64]),
                ConcreteType::UnionOf(vec![
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
                ]),
            ),
            // User struct — unchanged.
            (
                JuliaType::Struct("Foo".into()),
                ConcreteType::Struct {
                    name: "Foo".into(),
                    type_id: 0,
                },
            ),
            (
                JuliaType::Struct("Complex{Float64}".into()),
                ConcreteType::Struct {
                    name: "Complex{Float64}".into(),
                    type_id: 0,
                },
            ),
            // Containers — NOW structured (was `Any` before the reroute).
            (
                JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Float64]),
                ConcreteType::Tuple {
                    elements: vec![
                        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
                    ],
                },
            ),
            (
                JuliaType::Tuple,
                ConcreteType::Struct {
                    name: "Tuple".into(),
                    type_id: 0,
                },
            ),
            (
                JuliaType::Dict,
                ConcreteType::Dict {
                    key: Box::new(ConcreteType::Core(CoreType::Any)),
                    value: Box::new(ConcreteType::Core(CoreType::Any)),
                },
            ),
            (
                JuliaType::Set,
                ConcreteType::Set {
                    element: Box::new(ConcreteType::Core(CoreType::Any)),
                },
            ),
            (
                JuliaType::NamedTuple,
                ConcreteType::Struct {
                    name: "NamedTuple".into(),
                    type_id: 0,
                },
            ),
            (
                JuliaType::Function,
                ConcreteType::Function {
                    name: String::new(),
                },
            ),
            (
                JuliaType::UnitRange,
                ConcreteType::Range {
                    element: Box::new(ConcreteType::Core(CoreType::Any)),
                },
            ),
            (
                JuliaType::StepRange,
                ConcreteType::Range {
                    element: Box::new(ConcreteType::Core(CoreType::Any)),
                },
            ),
            // Abstract families WITHOUT a concrete ConcreteType image — stay `Any`.
            (JuliaType::Real, ConcreteType::Core(CoreType::Any)),
            (JuliaType::Signed, ConcreteType::Core(CoreType::Any)),
            (JuliaType::Unsigned, ConcreteType::Core(CoreType::Any)),
            (JuliaType::AbstractRange, ConcreteType::Core(CoreType::Any)),
        ];

        for (jt, expected) in &cases {
            assert_eq!(
                &julia_type_to_concrete_type_lossy(jt),
                expected,
                "post-reroute image for {jt:?}"
            );
        }
    }
}

/// Public helper function to convert LatticeType to ValueType.
///
/// This is a convenience function that delegates to the From implementation.
pub fn lattice_to_value_type(lattice_type: &LatticeType) -> ValueType {
    ValueType::from(lattice_type)
}

/// Extract a parametric `JuliaType` from a `LatticeType` when `ValueType` would lose info.
///
/// Currently handles `ConcreteType::Tuple { elements }` → `JuliaType::TupleOf(...)`.
/// Returns `None` when no parametric info would be lost by using `ValueType` alone.
/// (Issue #2317)
pub fn lattice_to_parametric_julia_type(lattice_type: &LatticeType) -> Option<JuliaType> {
    match lattice_type {
        LatticeType::Concrete(ConcreteType::Tuple { elements }) if !elements.is_empty() => {
            let julia_elements: Vec<JuliaType> =
                elements.iter().map(concrete_type_to_julia_type).collect();
            Some(JuliaType::TupleOf(julia_elements))
        }
        LatticeType::Concrete(ConcreteType::Array { element, ndims }) => {
            // Issue #6817: project the array rank for dispatch (see
            // `concrete_type_to_julia_type`). `Some(2)` → `Matrix`, higher ranks
            // → `Array{T,N}`, otherwise a 1-D `Vector`.
            let elem = concrete_type_to_julia_type(element);
            Some(match ndims {
                Some(2) => JuliaType::MatrixOf(Box::new(elem)),
                Some(n) if *n >= 3 => JuliaType::Struct(format!("Array{{{}, {}}}", elem.name(), n)),
                _ => JuliaType::VectorOf(Box::new(elem)),
            })
        }
        LatticeType::Concrete(ConcreteType::NamedTuple { fields }) => {
            Some(concrete_namedtuple_to_julia_type(fields))
        }
        LatticeType::Concrete(ConcreteType::Dict { key, value }) => {
            Some(JuliaType::Struct(format!(
                "Dict{{{},{}}}",
                concrete_type_parameter_name(key),
                concrete_type_parameter_name(value)
            )))
        }
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) if name.contains('{') => {
            Some(JuliaType::from_name_or_struct(name))
        }
        LatticeType::Concrete(ConcreteType::UnionOf(types)) => Some(JuliaType::Union(
            types.iter().map(concrete_type_to_julia_type).collect(),
        )),
        LatticeType::Union(types) => Some(JuliaType::Union(
            types.iter().map(concrete_type_to_julia_type).collect(),
        )),
        // Preserve `Union{}` (Bottom) faithfully for reflection callers
        // such as `Base.return_types` / `Base.infer_return_type`
        // (Issue #4679). This arm stays necessary: the codegen-direction
        // `lattice_to_value_type` deliberately widens `Bottom` to `Any`
        // (see the `From<&LatticeType>` impl, Issue #5916), so the lossy
        // fallback path cannot recover `Union{}`. `lattice_to_julia_type`
        // (the total variant) preserves Bottom the same way.
        LatticeType::Bottom => Some(JuliaType::Bottom),
        _ => None,
    }
}

/// Convert a `LatticeType` to a best-effort `JuliaType`.
///
/// Unlike [`lattice_to_parametric_julia_type`], this always produces a
/// `JuliaType` (falling back to `JuliaType::Any` for shapes that carry no
/// concrete info). It preserves parametric struct names spelled with braces
/// (e.g. `Foo{Int64}`) and concrete element types of tuples / arrays so that
/// reflection-time constructor inference can bind struct type parameters from
/// actual argument types (Issues #4849 / #4850 / #4851).
pub fn lattice_to_julia_type(lattice_type: &LatticeType) -> JuliaType {
    match lattice_type {
        LatticeType::Concrete(ct) => concrete_type_to_julia_type(ct),
        LatticeType::Const(value) => concrete_type_to_julia_type(&value.to_concrete_type()),
        LatticeType::Union(types) => {
            JuliaType::Union(types.iter().map(concrete_type_to_julia_type).collect())
        }
        // `Union{}` is representable in `JuliaType`, so the total conversion
        // preserves Bottom instead of widening it to `Any` (Issue #5916).
        LatticeType::Bottom => JuliaType::Bottom,
        _ => JuliaType::Any,
    }
}

/// Convert a `ConcreteType` to a `JuliaType`.
fn concrete_type_to_julia_type(ct: &ConcreteType) -> JuliaType {
    match ct {
        // Issue #6916 (epic #5916): every nullary primitive shares the
        // `CoreType` hub's mapping, so delegate instead of duplicating the
        // 21-entry primitive table here. Restricted to `Primitive(_)`:
        // abstracts / `Any` keep their historical widen-to-`Any` via the
        // catch-all below (the hub maps abstracts to their own `JuliaType`,
        // which would be a behaviour change). Pinned by
        // `concrete_type_to_julia_type_routes_primitives_through_core_hub_issue_6916`.
        ConcreteType::Core(core @ CoreType::Primitive(_)) => {
            crate::inference_core::core_type_to_julia_type(core)
        }
        ConcreteType::Tuple { elements } => {
            JuliaType::TupleOf(elements.iter().map(concrete_type_to_julia_type).collect())
        }
        ConcreteType::Array { element, ndims } => {
            // Issue #6817: project the array rank. `Some(1)`/`None` (unknown rank)
            // default to a 1-D `Vector`, `Some(2)` to a `Matrix`, higher ranks to
            // the explicit `Array{T,N}` so multi-dimensional results dispatch to
            // `::Matrix` / `::Array{T,N}` rather than collapsing to `::Vector`.
            let elem = concrete_type_to_julia_type(element);
            match ndims {
                Some(2) => JuliaType::MatrixOf(Box::new(elem)),
                Some(n) if *n >= 3 => JuliaType::Struct(format!("Array{{{}, {}}}", elem.name(), n)),
                _ => JuliaType::VectorOf(Box::new(elem)),
            }
        }
        ConcreteType::NamedTuple { fields } => concrete_namedtuple_to_julia_type(fields),
        ConcreteType::Dict { key, value } => JuliaType::Struct(format!(
            "Dict{{{},{}}}",
            concrete_type_parameter_name(key),
            concrete_type_parameter_name(value)
        )),
        ConcreteType::UnionOf(types) => {
            JuliaType::Union(types.iter().map(concrete_type_to_julia_type).collect())
        }
        // Issue #3511: TupleVararg falls back to a non-parametric `JuliaType`
        // since downstream codegen / VM does not yet model Vararg tails.
        // Inference still benefits from the precision internally.
        ConcreteType::TupleVararg { .. } => JuliaType::Any,
        // Issue #6599: unify the divergent `LatticeType → JuliaType` pair.
        // A braced parametric struct spelling (`Vector{Int64}`,
        // `Complex{Float64}`, …) is parsed through `from_name_or_struct` so the
        // total converter (`lattice_to_julia_type`) recovers the structured
        // `JuliaType` (`VectorOf`/`MatrixOf`/canonical struct) exactly like the
        // partial converter (`lattice_to_parametric_julia_type`, the braced
        // `Struct` arm) already does — eliminating the structure-preserving vs
        // string-opaque divergence. `from_name_or_struct` falls back to
        // `Struct(normalized)` for unknown braced spellings, and **bare**
        // (non-braced) names keep their opaque `Struct(name)` form so a struct
        // literally named e.g. `Int64`/`Number` is never reinterpreted as the
        // primitive (verified against upstream `julia` 1.12: `Vector{Int64}` is
        // a concrete parametric `DataType`, not an opaque name).
        ConcreteType::Struct { name, .. } if name.contains('{') => {
            JuliaType::from_name_or_struct(name)
        }
        ConcreteType::Struct { name, .. } => JuliaType::Struct(name.clone()),
        // A type object's type is `DataType` (e.g. `typeof(Int64) === DataType`).
        // Reflection-time inference binds a `where` type parameter to its concrete
        // `DataType` value, so a method returning that parameter must report
        // `DataType` rather than widening to `Any` (Issue #4843).
        ConcreteType::DataType { .. } => JuliaType::DataType,
        _ => JuliaType::Any,
    }
}

fn concrete_type_parameter_name(ct: &ConcreteType) -> String {
    concrete_type_to_julia_type(ct).name().to_string()
}

fn concrete_namedtuple_to_julia_type(fields: &[(String, ConcreteType)]) -> JuliaType {
    let field_parts = fields
        .iter()
        .map(|(name, ty)| {
            let field_ty = concrete_type_to_julia_type(ty);
            if matches!(field_ty, JuliaType::Any) {
                name.clone()
            } else {
                format!("{}::{}", name, field_ty.name())
            }
        })
        .collect::<Vec<_>>();
    JuliaType::Struct(format!("@NamedTuple{{{}}}", field_parts.join(", ")))
}
