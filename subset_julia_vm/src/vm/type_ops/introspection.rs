//! Type introspection: get_type_name, etc.

use crate::rng::{RngInstance, RngLike};
use crate::vm::value::{ArrayElementType, Value};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    /// Get the Julia type name for a value.
    ///
    /// Returns type names that match Julia's `typeof()` output.
    pub(in crate::vm) fn get_type_name(&self, val: &Value) -> String {
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
            Value::Array(arr) => {
                let arr = arr.borrow();
                let elem_type = match arr.element_type() {
                    ArrayElementType::F32 => "Float32",
                    ArrayElementType::F64 => "Float64",
                    ArrayElementType::ComplexF32 => "Complex{Float32}",
                    ArrayElementType::ComplexF64 => "Complex{Float64}",
                    ArrayElementType::I8 => "Int8",
                    ArrayElementType::I16 => "Int16",
                    ArrayElementType::I32 => "Int32",
                    ArrayElementType::I64 => "Int64",
                    ArrayElementType::U8 => "UInt8",
                    ArrayElementType::U16 => "UInt16",
                    ArrayElementType::U32 => "UInt32",
                    ArrayElementType::U64 => "UInt64",
                    ArrayElementType::Bool => "Bool",
                    ArrayElementType::String => "String",
                    ArrayElementType::Char => "Char",
                    ArrayElementType::StructOf(_) | ArrayElementType::StructInlineOf(_, _) => {
                        "Struct"
                    }
                    ArrayElementType::Struct
                    | ArrayElementType::Any
                    | ArrayElementType::TupleOf(_) => "Any",
                };
                match arr.shape.len() {
                    1 => format!("Vector{{{}}}", elem_type),
                    2 => format!("Matrix{{{}}}", elem_type),
                    n => format!("Array{{{}, {}}}", elem_type, n),
                }
            }
            Value::Range(r) => {
                if r.is_unit_range() {
                    "UnitRange{Int64}".to_string()
                } else {
                    "StepRange{Int64, Int64}".to_string()
                }
            }
            Value::SliceAll => "Colon".to_string(),
            // Complex is now a Pure Julia struct - preserve actual type parameter
            Value::Struct(s) => s.struct_name.clone(),
            Value::StructRef(idx) => {
                // Look up the struct in the struct_heap to get its name
                if let Some(s) = self.struct_heap.get(*idx) {
                    s.struct_name.clone()
                } else {
                    "Struct".to_string()
                }
            }
            Value::Rng(rng) => match rng {
                RngInstance::Stable(_) => "StableRNG".to_string(),
                RngInstance::Xoshiro(_) => "Xoshiro".to_string(),
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
            Value::Dict(d) => {
                let k = d.key_type.as_deref().unwrap_or("Any");
                let v = d.value_type.as_deref().unwrap_or("Any");
                format!("Dict{{{}, {}}}", k, v)
            }
            Value::Set(_) => "Set{Any}".to_string(),
            Value::Ref(inner) => {
                // Ref{T} wraps another value
                format!("Ref{{{}}}", self.get_type_name(inner))
            }
            Value::Generator(_) => "Base.Generator".to_string(),
            Value::DataType(_) => "DataType".to_string(),
            Value::Module(m) => format!("Module({})", m.name),
            Value::Function(_) => "Function".to_string(),
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
                mem.julia_type_name()
            }
        }
    }
}
