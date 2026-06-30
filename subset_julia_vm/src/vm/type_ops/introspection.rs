//! Type introspection: get_type_name, etc.

use crate::rng::{RngInstance, RngLike};
use crate::vm::value::{ArrayData, ArrayElementType, Value};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    /// Get the Julia type name for a value.
    ///
    /// Returns type names that match Julia's `typeof()` output.
    pub(in crate::vm) fn get_type_name(&self, val: &Value) -> String {
        // Route the legacy native-array carrier through the shared
        // `crate::vm::value::native_array_value_ref` helper so the match
        // below no longer holds a native-array arm (Issue #3908).
        if let Some(arr_ref) = crate::vm::value::native_array_value_ref(val) {
            let arr = arr_ref.borrow();
            if let Some(container_type) = arr.array_type_override() {
                return container_type.to_string();
            }
            let elem_type_owned: String = match arr.element_type() {
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
                // Issue #3574: `Vector{SubString{String}}` for split/rsplit results.
                ArrayElementType::SubString => "SubString{String}".to_string(),
                ArrayElementType::Char => "Char".to_string(),
                ArrayElementType::Symbol => "Symbol".to_string(),
                ArrayElementType::Nothing => "Nothing".to_string(),
                ArrayElementType::StructOf(type_id)
                | ArrayElementType::StructInlineOf(type_id, _) => self
                    .struct_defs
                    .get(type_id)
                    .map_or_else(|| "Any".to_string(), |def| def.name.clone()),
                ArrayElementType::Struct => match &arr.data {
                    ArrayData::StructRefs(struct_refs) => match struct_refs
                        .first()
                        .and_then(|idx| self.struct_heap.get(*idx))
                        .map(|instance| &*instance.struct_name)
                    {
                        Some(first_name)
                            if struct_refs.iter().all(|idx| {
                                self.struct_heap
                                    .get(*idx)
                                    .is_some_and(|instance| &*instance.struct_name == first_name)
                            }) =>
                        {
                            first_name.to_string()
                        }
                        _ => "Any".to_string(),
                    },
                    _ => "Any".to_string(),
                },
                ArrayElementType::Any => "Any".to_string(),
                ArrayElementType::TupleOf(_) => arr.element_type().julia_type_name(),
                // Issue #3549.
                ArrayElementType::UnionOf(ref members) => {
                    format!("Union{{{}}}", ArrayElementType::union_body_string(members))
                }
                ArrayElementType::Abstract(ref name) => name.clone(),
            };
            return match arr.shape.len() {
                1 => format!("Vector{{{}}}", elem_type_owned),
                2 => format!("Matrix{{{}}}", elem_type_owned),
                n => format!("Array{{{}, {}}}", elem_type_owned, n),
            };
        }
        match val {
            // Signed integers
            Value::I8(_) => "Int8".to_string(),
            Value::I16(_) => "Int16".to_string(),
            Value::I32(_) => "Int32".to_string(),
            Value::I64(_) => "Int64".to_string(),
            Value::I128(_) => "Int128".to_string(),
            Value::BigInt(_) => "BigInt".to_string(),
            // Unsigned integers
            Value::U8(_) => "UInt8".to_string(),
            Value::U16(_) => "UInt16".to_string(),
            Value::U32(_) => "UInt32".to_string(),
            Value::U64(_) => "UInt64".to_string(),
            Value::U128(_) => "UInt128".to_string(),
            // Boolean
            Value::Bool(_) => "Bool".to_string(),
            // Floating point
            Value::F16(_) => "Float16".to_string(),
            Value::F32(_) => "Float32".to_string(),
            Value::F64(_) => "Float64".to_string(),
            Value::BigFloat(_) => "BigFloat".to_string(),
            Value::Str(_) => "String".to_string(),
            Value::Char(_) => "Char".to_string(),
            Value::Nothing => "Nothing".to_string(),
            Value::Missing => "Missing".to_string(),
            Value::Range(r) => {
                // Issue #3550: respect the typed element tag when present.
                let elem_name = match r.element_type {
                    crate::vm::value::RangeElementType::Default => {
                        if r.is_float {
                            "Float64"
                        } else {
                            "Int64"
                        }
                    }
                    other => other.julia_type_name(),
                };
                let is_explicit_float =
                    matches!(
                        r.element_type,
                        crate::vm::value::RangeElementType::Float32
                            | crate::vm::value::RangeElementType::Float64
                    ) || (matches!(r.element_type, crate::vm::value::RangeElementType::Default)
                        && r.is_float);
                if is_explicit_float {
                    format!(
                        "StepRangeLen{{{e}, Base.TwicePrecision{{{e}}}, Base.TwicePrecision{{{e}}}, Int64}}",
                        e = elem_name
                    )
                } else if matches!(r.element_type, crate::vm::value::RangeElementType::Char) {
                    // Char ranges always report `StepRange{Char, Int64}`
                    // (Issue #4830).
                    "StepRange{Char, Int64}".to_string()
                } else if r.is_unit_range() {
                    format!("UnitRange{{{}}}", elem_name)
                } else {
                    format!("StepRange{{{e}, {e}}}", e = elem_name)
                }
            }
            Value::SliceAll => "Colon".to_string(),
            // Complex is now a Pure Julia struct - preserve actual type parameter
            Value::Struct(s) => self
                .array_wrapper_julia_type_resolved(s)
                .map(|jt| jt.name().to_string())
                .unwrap_or_else(|| s.struct_name.to_string()),
            Value::StructRef(idx) => {
                // Look up the struct in the struct_heap to get its name
                if let Some(s) = self.struct_heap.get(*idx) {
                    self.array_wrapper_julia_type_resolved(s)
                        .map(|jt| jt.name().to_string())
                        .unwrap_or_else(|| s.struct_name.to_string())
                } else {
                    "Struct".to_string()
                }
            }
            Value::Rng(rng) => match rng {
                RngInstance::Stable(_) => "StableRNG".to_string(),
                RngInstance::Xoshiro(_) => "Xoshiro".to_string(),
                RngInstance::Mersenne(_) => "MersenneTwister".to_string(),
                // The global RNG handle (Random.default_rng()/GLOBAL_RNG)
                // reports as TaskLocalRNG to match upstream (Issue #7230).
                RngInstance::Global => "TaskLocalRNG".to_string(),
            },
            Value::Tuple(t) => {
                // Julia shows Tuple{T1, T2, ...}
                let types: Vec<String> = t.elements.iter().map(|e| self.get_type_name(e)).collect();
                format!("Tuple{{{}}}", types.join(", "))
            }
            Value::NamedTuple(nt) => {
                // Julia shows NamedTuple{(:a, :b), Tuple{T1, T2}}
                let names: Vec<String> = nt.names.iter().map(|n| format!(":{}", n)).collect();
                let types: Vec<String> = nt.values.iter().map(|v| self.get_type_name(v)).collect();
                format!(
                    "NamedTuple{{({}), Tuple{{{}}}}}",
                    names.join(", "),
                    types.join(", ")
                )
            }
            Value::Ref(inner) => {
                // Base.RefValue{T} wraps another value (Issue #5130)
                let v = inner.borrow();
                format!("Base.RefValue{{{}}}", self.get_type_name(&v))
            }
            Value::Generator(_) => "Base.Generator".to_string(),
            Value::DataType(_) => "DataType".to_string(),
            Value::RuntimeTypeVar(_) => "TypeVar".to_string(),
            Value::RuntimeTypeName(_) => "Core.TypeName".to_string(),
            // Every module value has runtime type `Module` (Issue #5005).
            // Embedding the module's own name here (e.g. `Module(Base)`) made the
            // dispatch type name fail to match a `::Module` parameter annotation,
            // so an untyped parameter wrongly won specificity.
            Value::Module(_) => "Module".to_string(),
            Value::Function(f) => format!("typeof({})", f.name),
            Value::Closure(_) => "Function".to_string(), // Closures are Functions
            Value::ComposedFunction(_) => "ComposedFunction".to_string(),
            Value::Undef => "#undef".to_string(),
            Value::IO(_) => "IOBuffer".to_string(),
            // Macro system types
            Value::Symbol(_) => "Symbol".to_string(),
            Value::Expr(_) => "Expr".to_string(),
            Value::QuoteNode(_) => "QuoteNode".to_string(),
            Value::LineNumberNode(_) => "LineNumberNode".to_string(),
            Value::GlobalRef(_) => "GlobalRef".to_string(),
            // Base.Pairs type (for kwargs...)
            Value::Pairs(p) => {
                // Julia shows Base.Pairs{Symbol, T, ...}
                let types: Vec<String> = p
                    .data
                    .values
                    .iter()
                    .map(|v| self.get_type_name(v))
                    .collect();
                if types.is_empty() {
                    "Base.Pairs{Symbol, Union{}, ...}".to_string()
                } else {
                    format!(
                        "Base.Pairs{{Symbol, {}, ...}}",
                        types.first().unwrap_or(&"Any".to_string())
                    )
                }
            }
            // Regex types
            Value::Regex(_) => "Regex".to_string(),
            Value::RegexMatch(_) => "RegexMatch".to_string(),
            // Enum type
            Value::Enum { type_name, .. } => type_name.clone(),
            // Memory{T} flat typed buffer
            Value::Memory(mem) => {
                let mem = mem.borrow();
                format!(
                    "Memory{{{}}}",
                    self.memory_element_type_name(mem.element_type())
                )
            }
            Value::MemoryRef(memref) => memref.julia_type_name(),
            // StaticArray flat representations carry their Julia type in their
            // metadata; report the exact parametric name for dispatch so methods
            // like `Size(x::StaticArray)` / `size(x::SMatrix{M,N,T})` can
            // resolve the abstract-type subtype relationship (Issue #7964).
            Value::StaticArray(sv) => sv.julia_type_name().to_string(),
            Value::StaticArrayInline(sv) => sv.julia_type_name_owned().to_string(),
            // The legacy native-array carrier is filtered out by the
            // early-return above (Issue #3908). This wildcard satisfies
            // Rust's exhaustiveness checking and provides a safe default
            // for any future `Value` variant: return "Any".
            _ => "Any".to_string(),
        }
    }

    /// Render the inner element-type name of a `Memory{T}` buffer, resolving a
    /// `StructOf`/`StructInlineOf` element tag back to the user struct's name via
    /// `struct_defs` (Issue #7304). `ArrayElementType::julia_type_name()` cannot
    /// do this lookup itself (it has no registry) and reports `Any`, which would
    /// widen `Memory{T}` / `Vector{T}` for a user struct `T` back to `Any`.
    pub(in crate::vm) fn memory_element_type_name(&self, elem_type: &ArrayElementType) -> String {
        match elem_type {
            ArrayElementType::StructOf(type_id) | ArrayElementType::StructInlineOf(type_id, _) => {
                self.struct_defs
                    .get(*type_id)
                    .map_or_else(|| "Any".to_string(), |def| def.name.clone())
            }
            other => other.julia_type_name(),
        }
    }

    /// Map an array `ArrayElementType` to a `JuliaType`, resolving a `StructOf`/
    /// `StructInlineOf` user-struct tag to `JuliaType::Struct(name)` via
    /// `struct_defs` (Issue #7304). The registry-free
    /// `array_element_type_to_julia_type` reports `Any` for these tags.
    pub(in crate::vm) fn array_element_type_to_julia_type_resolved(
        &self,
        elem_type: &ArrayElementType,
    ) -> crate::types::JuliaType {
        match elem_type {
            ArrayElementType::StructOf(type_id) | ArrayElementType::StructInlineOf(type_id, _) => {
                self.struct_defs.get(*type_id).map_or_else(
                    || crate::types::JuliaType::Any,
                    |def| crate::types::JuliaType::Struct(def.name.clone()),
                )
            }
            other => crate::vm::value::array_element_type_to_julia_type(other),
        }
    }

    /// `JuliaType` of an array-wrapper struct (`Vector{T}` / `Matrix{T}` /
    /// `Array{T,N}`), resolving a user-struct element tag via `struct_defs`
    /// (Issue #7304). Falls back to the registry-free
    /// `StructInstance::array_wrapper_julia_type` when the wrapper carries the
    /// legacy native-array carrier instead of a `Memory`/`MemoryRef`.
    pub(in crate::vm) fn array_wrapper_julia_type_resolved(
        &self,
        s: &crate::vm::value::StructInstance,
    ) -> Option<crate::types::JuliaType> {
        if let Some((elem_type, ndims)) = s.array_wrapper_element_array_type() {
            let elem = self.array_element_type_to_julia_type_resolved(&elem_type);
            return Some(crate::vm::value::julia_array_type_for_ndims(elem, ndims));
        }
        s.array_wrapper_julia_type()
    }
}
