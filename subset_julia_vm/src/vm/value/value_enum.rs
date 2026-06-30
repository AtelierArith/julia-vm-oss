//! Value - The main runtime value type for the Julia VM.
//!
//! This module contains:
//! - `Value`: The main enum representing all Julia values at runtime
//! - `ValueType`: A simplified type tag for Value variants

use crate::rng::RngInstance;
use half::f16;
use std::cell::RefCell;
use std::rc::Rc;

use super::array_element::{
    array_element_type_to_julia_type, julia_array_type_for_ndims, ArrayElementType,
};
use super::composed_function::ComposedFunctionValue;
use super::expr::ExprValue;
use super::generator::GeneratorValue;
use super::io::IORef;
use super::macro_::{GlobalRefValue, LineNumberNodeValue, SymbolValue};
use super::memory_value::{MemoryRef, MemoryRefValue};
use super::metadata::{ClosureValue, FunctionValue, ModuleValue};
use super::named_tuple::NamedTupleValue;
use super::pairs::PairsValue;
use super::range::{RangeElementType, RangeValue};
use super::regex::{RegexMatchValue, RegexValue};
use super::static_real::StaticRealValue;
use super::struct_instance::StructInstance;
use super::tuple::TupleValue;
use super::{ArrayRef, RustBigFloat, RustBigInt, BIGFLOAT_PRECISION};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeTypeVarValue {
    pub id: u64,
    pub name: String,
    pub lower_bound: crate::types::JuliaType,
    pub upper_bound: crate::types::JuliaType,
}

impl RuntimeTypeVarValue {
    pub fn projection(&self) -> crate::types::JuliaType {
        let upper = if matches!(self.upper_bound, crate::types::JuliaType::Any) {
            None
        } else {
            Some(self.upper_bound.name().to_string())
        };
        crate::types::JuliaType::TypeVar(self.name.clone(), upper)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeTypeNameValue {
    pub name: String,
}

/// Interior-mutable reference cell backing `Base.RefValue{T}` (Issue #5130).
///
/// `Rc<RefCell<Value>>` gives `Ref` proper reference semantics: `r[] = v`
/// mutates the boxed value in place, and aliases observe the update — matching
/// upstream Julia's mutable single-element `RefValue` container. It also keeps
/// serving as the broadcast scalar wrapper (`isa(x, Ref)` is true).
pub type RefCellRef = Rc<RefCell<Value>>;

/// Construct a fresh `Ref` value wrapping `inner` (Issue #5130).
pub fn new_ref(inner: Value) -> Value {
    Value::Ref(Rc::new(RefCell::new(inner)))
}

#[derive(Debug, Clone)]
pub enum Value {
    // Signed integers
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    BigInt(RustBigInt), // Arbitrary precision integer
    // Unsigned integers
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    // Boolean
    Bool(bool),
    // Floating point
    F16(f16),
    F32(f32),
    F64(f64),
    BigFloat(RustBigFloat), // Arbitrary precision float
    // String types
    Str(String),
    Char(char), // Julia's Char type (32-bit Unicode codepoint)
    Nothing,    // Julia's `nothing` value (singleton of type Nothing)
    Missing,    // Julia's `missing` value (singleton of type Missing)
    Undef,      // Julia's #undef - uninitialized field value
    // Reference-counted (`Rc<RefCell<ArrayValue>>`) mutable array carrier. Its
    // sole runtime origin is `expr.args` — the mutable `Vector{Any}` AST args of
    // an `Expr`, which need auto-freed reference semantics (`struct_heap` has no
    // per-value GC, so a heap-`StructRef` wrapper would leak one slot per
    // transient `Expr` node). All *general* array values are the MemoryRef-backed
    // pure-Julia `Array{T,N}` wrapper; this carrier is confined to the `expr.args`
    // representation (and the generic array ops a `Vector{Any}` flows through).
    // Accessed via the `native_array_*` carrier helpers in `value/array_value`.
    // (Renamed from `NativeArray`, Issue #6807.)
    ExprArgs(ArrayRef),
    Memory(MemoryRef),                      // Flat typed memory buffer (Memory{T})
    MemoryRef(Box<MemoryRefValue>),         // Offset reference into Memory{T} (MemoryRef{T})
    Range(RangeValue),                      // Lazy range (start:step:stop)
    SliceAll,                               // ':' slice marker for indexing
    Struct(StructInstance),                 // User-defined struct (immutable), also Complex numbers
    StructRef(usize),                       // Mutable struct reference (heap index)
    Rng(RngInstance),                       // RNG instance (StableRNG/Xoshiro/MersenneTwister)
    Tuple(TupleValue),                      // Immutable tuple
    SimpleVector(TupleValue), // Core.SimpleVector (svec) - returned by <DataType>.parameters (Issue #4722)
    NamedTuple(NamedTupleValue), // Named tuple
    Pairs(PairsValue),        // Base.Pairs (for kwargs...)
    Ref(RefCellRef), // Base.RefValue{T} - mutable single-element box (Issue #5130); also protects value from broadcasting (treated as scalar)
    Generator(Box<GeneratorValue>), // Lazy generator (boxed: 104->8 bytes, Issue #5171)
    DataType(Box<crate::types::JuliaType>), // DataType (boxed: 56->8 bytes, Issue #7977/#7966)
    RuntimeTypeVar(Box<RuntimeTypeVarValue>), // Fresh TypeVar object with identity
    RuntimeTypeName(Box<RuntimeTypeNameValue>), // TypeName identity exposed by DataType.name (Issue #8451)
    Module(Box<ModuleValue>),                   // Julia module (boxed: 72->8 bytes)
    Function(FunctionValue),                    // Julia function object
    Closure(ClosureValue),                      // Julia closure with captured variables
    ComposedFunction(ComposedFunctionValue),    // Composed function (f ∘ g)
    IO(IORef), // IO stream for print/show operations (interior mutability)
    // Macro system types
    Symbol(SymbolValue),   // Julia Symbol (:foo) - quoted identifier
    Expr(ExprValue),       // Julia Expr - AST node for metaprogramming
    QuoteNode(Box<Value>), // QuoteNode - wraps a value that shouldn't be evaluated
    LineNumberNode(LineNumberNodeValue), // LineNumberNode - source location debug info
    GlobalRef(GlobalRefValue), // GlobalRef - reference to global variable in a module
    // Regex types
    Regex(Box<RegexValue>), // Julia Regex (boxed: 56->8 bytes, Issue #7966)
    RegexMatch(Box<RegexMatchValue>), // Julia RegexMatch (boxed: 80->8 bytes)
    // Enum type (from @enum macro)
    Enum {
        type_name: String, // The enum type (e.g., "Color")
        value: i64,        // The integer value
    },
    // Flat representation for small SVector{N,T} / SMatrix{M,N,T}
    // (Issue #7964 Phase 1). Eliminates heap Vec<Value> tuple boxing and
    // struct_heap growth in hot loops.
    StaticArray(Box<StaticRealValue>),
    // Zero-allocation inline storage for small N≤4 StaticArrays (Issue #7964
    // Phase 3). `StaticArrayInlineData` is `Copy` (40-byte payload), so no
    // heap allocation occurs on push/pop. Supersedes `StaticArray` for N≤4.
    StaticArrayInline(crate::vm::value::static_real::StaticArrayInlineData),
}

/// Helper enum for serializing the subset of Value variants that are serializable.
/// Used for Base cache kwarg defaults and other contexts where only literal values appear.
#[derive(serde::Serialize, serde::Deserialize)]
enum SerializableValue {
    Nothing,
    Missing,
    Undef,
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    F16(f16),
    F32(f32),
    F64(f64),
    Str(String),
    Char(char),
    Symbol(String),
    Enum { type_name: String, value: i64 },
}

impl serde::Serialize for Value {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let sv = match self {
            Value::Nothing => SerializableValue::Nothing,
            Value::Missing => SerializableValue::Missing,
            Value::Undef => SerializableValue::Undef,
            Value::Bool(v) => SerializableValue::Bool(*v),
            Value::I8(v) => SerializableValue::I8(*v),
            Value::I16(v) => SerializableValue::I16(*v),
            Value::I32(v) => SerializableValue::I32(*v),
            Value::I64(v) => SerializableValue::I64(*v),
            Value::I128(v) => SerializableValue::I128(*v),
            Value::U8(v) => SerializableValue::U8(*v),
            Value::U16(v) => SerializableValue::U16(*v),
            Value::U32(v) => SerializableValue::U32(*v),
            Value::U64(v) => SerializableValue::U64(*v),
            Value::U128(v) => SerializableValue::U128(*v),
            Value::F16(v) => SerializableValue::F16(*v),
            Value::F32(v) => SerializableValue::F32(*v),
            Value::F64(v) => SerializableValue::F64(*v),
            Value::Str(v) => SerializableValue::Str(v.clone()),
            Value::Char(v) => SerializableValue::Char(*v),
            Value::Symbol(v) => SerializableValue::Symbol(v.as_str().to_string()),
            Value::Enum { type_name, value } => SerializableValue::Enum {
                type_name: type_name.clone(),
                value: *value,
            },
            other => {
                return Err(serde::ser::Error::custom(format!(
                    "Cannot serialize Value variant: {:?}",
                    other.value_type()
                )));
            }
        };
        sv.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Value {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let sv = SerializableValue::deserialize(deserializer)?;
        Ok(match sv {
            SerializableValue::Nothing => Value::Nothing,
            SerializableValue::Missing => Value::Missing,
            SerializableValue::Undef => Value::Undef,
            SerializableValue::Bool(v) => Value::Bool(v),
            SerializableValue::I8(v) => Value::I8(v),
            SerializableValue::I16(v) => Value::I16(v),
            SerializableValue::I32(v) => Value::I32(v),
            SerializableValue::I64(v) => Value::I64(v),
            SerializableValue::I128(v) => Value::I128(v),
            SerializableValue::U8(v) => Value::U8(v),
            SerializableValue::U16(v) => Value::U16(v),
            SerializableValue::U32(v) => Value::U32(v),
            SerializableValue::U64(v) => Value::U64(v),
            SerializableValue::U128(v) => Value::U128(v),
            SerializableValue::F16(v) => Value::F16(v),
            SerializableValue::F32(v) => Value::F32(v),
            SerializableValue::F64(v) => Value::F64(v),
            SerializableValue::Str(v) => Value::Str(v),
            SerializableValue::Char(v) => Value::Char(v),
            SerializableValue::Symbol(v) => Value::Symbol(SymbolValue::new(v)),
            SerializableValue::Enum { type_name, value } => Value::Enum { type_name, value },
        })
    }
}

impl Value {
    /// Get the runtime type of this value as a JuliaType.
    pub fn runtime_type(&self) -> crate::types::JuliaType {
        // Route the legacy native-array carrier through the shared
        // `super::array_value::native_array_value_ref` helper so the match
        // below no longer holds a native-array arm (Issue #3908).
        if let Some(arr_ref) = super::array_value::native_array_value_ref(self) {
            let arr = arr_ref.borrow();
            if let Some(container_type) = arr.array_type_override() {
                return crate::types::JuliaType::Struct(container_type.to_string());
            }
            let elem_type = array_element_type_to_julia_type(&arr.element_type());
            return julia_array_type_for_ndims(elem_type, arr.shape.len());
        }
        match self {
            // Signed integers
            Value::I8(_) => crate::types::JuliaType::Int8,
            Value::I16(_) => crate::types::JuliaType::Int16,
            Value::I32(_) => crate::types::JuliaType::Int32,
            Value::I64(_) => crate::types::JuliaType::Int64,
            Value::I128(_) => crate::types::JuliaType::Int128,
            Value::BigInt(_) => crate::types::JuliaType::BigInt,
            // Unsigned integers
            Value::U8(_) => crate::types::JuliaType::UInt8,
            Value::U16(_) => crate::types::JuliaType::UInt16,
            Value::U32(_) => crate::types::JuliaType::UInt32,
            Value::U64(_) => crate::types::JuliaType::UInt64,
            Value::U128(_) => crate::types::JuliaType::UInt128,
            // Boolean
            Value::Bool(_) => crate::types::JuliaType::Bool,
            // Floating point
            Value::F16(_) => crate::types::JuliaType::Float16,
            Value::F32(_) => crate::types::JuliaType::Float32,
            Value::F64(_) => crate::types::JuliaType::Float64,
            Value::BigFloat(_) => crate::types::JuliaType::BigFloat,
            Value::Str(_) => crate::types::JuliaType::String,
            Value::Char(_) => crate::types::JuliaType::Char,
            Value::Nothing => crate::types::JuliaType::Nothing,
            Value::Missing => crate::types::JuliaType::Missing,
            Value::Undef => crate::types::JuliaType::Any, // #undef has no type
            Value::Memory(mem) => {
                let mem = mem.borrow();
                let elem_type_name = mem.element_type().julia_type_name();
                crate::types::JuliaType::Struct(format!("Memory{{{}}}", elem_type_name))
            }
            Value::MemoryRef(memref) => crate::types::JuliaType::Struct(memref.julia_type_name()),
            Value::Range(r) => {
                // Issue #3550: report `UnitRange{T}` / `StepRange{T,T}` parametric
                // types when the range carries a typed element tag, matching
                // official Julia.
                let elem_name = match r.element_type {
                    RangeElementType::Default => {
                        if r.is_float {
                            "Float64"
                        } else {
                            "Int64"
                        }
                    }
                    other => other.julia_type_name(),
                };
                let is_explicit_float = matches!(
                    r.element_type,
                    RangeElementType::Float32 | RangeElementType::Float64
                ) || (matches!(r.element_type, RangeElementType::Default)
                    && r.is_float);
                if is_explicit_float {
                    crate::types::JuliaType::Struct(format!(
                        "StepRangeLen{{{e}, Base.TwicePrecision{{{e}}}, Base.TwicePrecision{{{e}}}, Int64}}",
                        e = elem_name
                    ))
                } else if matches!(r.element_type, RangeElementType::Char) {
                    // Char ranges in upstream Julia are always
                    // `StepRange{Char, Int64}` (never `UnitRange{Char}`),
                    // because `:` over non-numeric types defaults to the
                    // explicit-step form and Char arithmetic uses Int
                    // steps (Char + Int = Char). Reporting
                    // `UnitRange{Char}` for the step-1 case caused
                    // `repr('a':'e')` to dispatch to `show(::UnitRange)`
                    // and emit `'a':'e'` instead of `'a':1:'e'`
                    // (Issue #4830, follow-up to #4795).
                    crate::types::JuliaType::Struct("StepRange{Char, Int64}".to_string())
                } else if r.is_unit_range() {
                    crate::types::JuliaType::Struct(format!("UnitRange{{{}}}", elem_name))
                } else {
                    crate::types::JuliaType::Struct(format!(
                        "StepRange{{{}, {}}}",
                        elem_name, elem_name
                    ))
                }
            }
            Value::SliceAll => crate::types::JuliaType::Any,
            Value::Struct(s) => {
                // Complex numbers are now Pure Julia structs, not a primitive type
                if let Some(array_type) = s.array_wrapper_julia_type() {
                    array_type
                } else if s.struct_name.is_empty() {
                    crate::types::JuliaType::Any
                } else {
                    crate::types::JuliaType::Struct(s.struct_name.to_string())
                }
            }
            Value::StructRef(_) => crate::types::JuliaType::Any, // StructRef needs VM context to resolve
            // typeof(rng): report the concrete RNG type. The global handle
            // (default_rng()/GLOBAL_RNG) reports as TaskLocalRNG (Issues #7230, #7231).
            Value::Rng(rng) => crate::types::JuliaType::Struct(
                match rng {
                    RngInstance::Stable(_) => "StableRNG",
                    RngInstance::Xoshiro(_) => "Xoshiro",
                    RngInstance::Mersenne(_) => "MersenneTwister",
                    RngInstance::Global => "TaskLocalRNG",
                }
                .to_string(),
            ),
            Value::Tuple(t) => {
                let element_types: Vec<crate::types::JuliaType> =
                    t.elements.iter().map(|e| e.runtime_type()).collect();
                crate::types::JuliaType::TupleOf(element_types)
            }
            // Issue #4722: typeof(<DataType>.parameters) === Core.SimpleVector.
            Value::SimpleVector(_) => {
                crate::types::JuliaType::Struct("Core.SimpleVector".to_string())
            }
            Value::NamedTuple(_) => crate::types::JuliaType::NamedTuple,
            Value::Ref(inner) => {
                // Base.RefValue{T}: typeof(Ref(5)) === Base.RefValue{Int64} (Issue #5130)
                let inner_ty = inner.borrow().runtime_type();
                crate::types::JuliaType::Struct(format!("Base.RefValue{{{}}}", inner_ty))
            }
            Value::Generator(_) => crate::types::JuliaType::Generator, // Generator type
            Value::DataType(_) => crate::types::JuliaType::DataType, // typeof(typeof(x)) == DataType
            Value::RuntimeTypeVar(_) => crate::types::JuliaType::Struct("TypeVar".to_string()),
            Value::RuntimeTypeName(_) => {
                crate::types::JuliaType::Struct("Core.TypeName".to_string())
            }
            Value::Module(_) => crate::types::JuliaType::Module, // typeof(Statistics) == Module
            Value::Function(f) => crate::types::JuliaType::Struct(format!("typeof({})", f.name)), // callable singleton <: Function
            Value::Closure(_) => crate::types::JuliaType::Function, // Closures are also Functions
            Value::ComposedFunction(_) => crate::types::JuliaType::Function, // Composed functions are also Functions
            Value::IO(_) => crate::types::JuliaType::IOBuffer, // IO stream type (concrete)
            // Macro system types
            Value::Symbol(_) => crate::types::JuliaType::Symbol,
            Value::Expr(_) => crate::types::JuliaType::Expr,
            Value::QuoteNode(_) => crate::types::JuliaType::QuoteNode,
            Value::LineNumberNode(_) => crate::types::JuliaType::LineNumberNode,
            Value::GlobalRef(_) => crate::types::JuliaType::GlobalRef,
            Value::Pairs(_) => crate::types::JuliaType::Pairs,
            Value::Regex(_) => crate::types::JuliaType::Struct("Regex".to_string()),
            Value::RegexMatch(_) => crate::types::JuliaType::Struct("RegexMatch".to_string()),
            Value::Enum { type_name, .. } => crate::types::JuliaType::Enum(type_name.clone()),
            // Flat static-array representation (Issue #7964): reports the same
            // concrete Julia type as the Struct+Tuple representation it replaces.
            Value::StaticArray(sv) => {
                crate::types::JuliaType::Struct(sv.julia_type_name().to_string())
            }
            Value::StaticArrayInline(sv) => {
                crate::types::JuliaType::Struct(sv.julia_type_name_owned().to_string())
            }
            // The legacy native-array carrier is filtered out by the
            // early-return above (Issue #3908). This wildcard satisfies
            // Rust's exhaustiveness checking and provides a safe default for
            // any future `Value` variant: return `Any`.
            _ => crate::types::JuliaType::Any,
        }
    }

    /// Get the ValueType of this value.
    pub fn value_type(&self) -> ValueType {
        // Route the legacy native-array carrier through the shared
        // `super::array_value::native_array_value_ref` helper so the match
        // below no longer holds a native-array arm (Issue #3908).
        if super::array_value::is_native_array_value(self) {
            return ValueType::Array;
        }
        match self {
            // Signed integers
            Value::I8(_) => ValueType::I8,
            Value::I16(_) => ValueType::I16,
            Value::I32(_) => ValueType::I32,
            Value::I64(_) => ValueType::I64,
            Value::I128(_) => ValueType::I128,
            Value::BigInt(_) => ValueType::BigInt,
            // Unsigned integers
            Value::U8(_) => ValueType::U8,
            Value::U16(_) => ValueType::U16,
            Value::U32(_) => ValueType::U32,
            Value::U64(_) => ValueType::U64,
            Value::U128(_) => ValueType::U128,
            // Boolean
            Value::Bool(_) => ValueType::Bool,
            // Floating point
            Value::F16(_) => ValueType::F16,
            Value::F32(_) => ValueType::F32,
            Value::F64(_) => ValueType::F64,
            Value::BigFloat(_) => ValueType::BigFloat,
            // String types
            Value::Str(_) => ValueType::Str,
            Value::Char(_) => ValueType::Char,
            // Special types
            Value::Nothing => ValueType::Nothing,
            Value::Missing => ValueType::Missing,
            Value::Undef => ValueType::Any, // #undef has no specific type
            Value::Memory(ref m) => ValueType::MemoryOf(m.borrow().element_type.clone()),
            Value::MemoryRef(_) => ValueType::Any,
            Value::Range(_) => ValueType::Range,
            Value::SliceAll => ValueType::Array,
            Value::Struct(s) => value_type_for_struct_instance(s),
            Value::StructRef(_) => ValueType::Any, // StructRef type is dynamic
            Value::Rng(_) => ValueType::Rng,
            Value::Tuple(_) => ValueType::Tuple,
            Value::NamedTuple(_) => ValueType::NamedTuple,
            // Coarse ValueType tag: Ref keeps reporting the inner value's tag so the
            // existing broadcast/dispatch special-casing of `Value::Ref(_)` keeps working.
            // The precise `Base.RefValue{T}` type is reported by `runtime_type()` (Issue #5130).
            Value::Ref(inner) => inner.borrow().value_type(),
            Value::Generator(_) => ValueType::Generator,
            Value::DataType(_) | Value::RuntimeTypeVar(_) => ValueType::DataType,
            Value::RuntimeTypeName(_) => ValueType::Any,
            Value::Module(_) => ValueType::Module,
            Value::Function(_) => ValueType::Function,
            Value::Closure(_) => ValueType::Function, // Closures are Functions at the type level
            Value::ComposedFunction(_) => ValueType::Function,
            Value::IO(_) => ValueType::IO,
            // Macro system types
            Value::Symbol(_) => ValueType::Symbol,
            Value::Expr(_) => ValueType::Expr,
            Value::QuoteNode(_) => ValueType::QuoteNode,
            Value::LineNumberNode(_) => ValueType::LineNumberNode,
            Value::GlobalRef(_) => ValueType::GlobalRef,
            // Pairs type (for kwargs...)
            Value::Pairs(_) => ValueType::Pairs,
            // Regex types
            Value::Regex(_) => ValueType::Regex,
            Value::RegexMatch(_) => ValueType::RegexMatch,
            // Enum type
            Value::Enum { .. } => ValueType::Enum,
            // Flat static-array: treated as a struct-like value for dispatch
            // (Issue #7964). The precise type is available via runtime_type().
            Value::StaticArray(_) | Value::StaticArrayInline(_) => ValueType::Any,
            // The legacy native-array carrier is filtered out by the
            // early-return above (Issue #3908). This wildcard satisfies
            // Rust's exhaustiveness checking and provides a safe default for
            // any future `Value` variant: return `Any`.
            _ => ValueType::Any,
        }
    }

    /// Check if this value is a complex number (Complex struct)
    pub fn is_complex(&self) -> bool {
        match self {
            Value::Struct(s) => s.is_complex(),
            _ => false,
        }
    }

    /// Extract (re, im) from a complex value (Complex struct)
    /// Returns None if not a complex value
    /// Note: Also returns Some for I64/F64 values (promoted to complex with im=0)
    pub fn as_complex_parts(&self) -> Option<(f64, f64)> {
        match self {
            Value::Struct(s) => s.as_complex_parts(),
            Value::I64(v) => Some((*v as f64, 0.0)),
            Value::F64(v) => Some((*v, 0.0)),
            _ => None,
        }
    }

    /// Create a Complex struct value from (re, im) with specified type_id
    pub fn complex_struct(type_id: usize, re: f64, im: f64) -> Self {
        Value::Struct(StructInstance::complex(type_id, re, im))
    }

    /// Create a Complex struct value from (re, im) with specified type_id
    /// Note: type_id must be looked up from struct_table at runtime
    pub fn new_complex(type_id: usize, re: f64, im: f64) -> Self {
        Value::Struct(StructInstance::new_complex(type_id, re, im))
    }

    /// Create a BigInt value from an i64
    pub fn bigint_from_i64(v: i64) -> Self {
        Value::BigInt(RustBigInt::from(v))
    }

    /// Create a BigInt value from a string.
    ///
    /// Returns the BigInt value, or falls back to BigInt(0) if parsing fails.
    pub fn new_bigint(s: &str) -> Self {
        Value::BigInt(s.parse::<RustBigInt>().unwrap_or_default())
    }

    /// Check if this value is a BigInt
    pub fn is_bigint(&self) -> bool {
        matches!(self, Value::BigInt(_))
    }

    /// Get the BigInt value if this is a BigInt, otherwise None
    pub fn as_bigint(&self) -> Option<&RustBigInt> {
        match self {
            Value::BigInt(v) => Some(v),
            _ => None,
        }
    }

    /// Create a BigFloat from an f64 value
    pub fn bigfloat_from_f64(val: f64) -> Value {
        let bf = RustBigFloat::from_f64(val, BIGFLOAT_PRECISION);
        Value::BigFloat(bf)
    }

    /// Create a new BigFloat value from a BigFloat
    pub fn new_bigfloat(bf: RustBigFloat) -> Value {
        Value::BigFloat(bf)
    }

    /// Check if this value is a BigFloat
    pub fn is_bigfloat(&self) -> bool {
        matches!(self, Value::BigFloat(_))
    }

    /// Get the BigFloat value if this is a BigFloat, otherwise None
    pub fn as_bigfloat(&self) -> Option<&RustBigFloat> {
        match self {
            Value::BigFloat(v) => Some(v),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::RngInstance;
    use crate::vm::value::io::IOValue;
    use crate::vm::value::GeneratorCallable;

    /// Compile-time coverage test for ALL Value variants (Issue #1736).
    ///
    /// This test constructs every `Value` variant and performs basic operations
    /// (Debug format + runtime_type). If a new variant is added to the `Value`
    /// enum and not included here, this test will **fail to compile** due to the
    /// exhaustive match at the end.
    ///
    /// When adding a new Value variant, you MUST add it to this test.
    #[test]
    fn test_all_value_variants_constructed() {
        let tuple_array_ref = super::super::new_array_ref(super::super::ArrayValue::new(
            super::super::ArrayData::Any(vec![]),
            vec![0],
        ));
        tuple_array_ref.borrow_mut().element_type_override = Some(ArrayElementType::TupleOf(vec![
            ArrayElementType::ComplexF64,
        ]));

        let all_values: Vec<Value> = vec![
            // Signed integers
            Value::I8(0),
            Value::I16(0),
            Value::I32(0),
            Value::I64(0),
            Value::I128(0),
            Value::BigInt(RustBigInt::from(0)),
            // Unsigned integers
            Value::U8(0),
            Value::U16(0),
            Value::U32(0),
            Value::U64(0),
            Value::U128(0),
            // Boolean
            Value::Bool(false),
            // Floating point
            Value::F16(f16::from_f32(0.0)),
            Value::F32(0.0),
            Value::F64(0.0),
            Value::BigFloat(RustBigFloat::from_f64(0.0, BIGFLOAT_PRECISION)),
            // String types
            Value::Str(String::new()),
            Value::Char('a'),
            // Singleton types
            Value::Nothing,
            Value::Missing,
            Value::Undef,
            // Collections - route through the shared `native_array_ref_value`
            // constructor so this test holds no literal native-array
            // construction (Issue #3908).
            super::super::array_value::native_array_ref_value(tuple_array_ref),
            Value::Memory(super::super::new_memory_ref(
                super::super::MemoryValue::undef_typed(&super::super::ArrayElementType::F64, 0),
            )),
            Value::MemoryRef(Box::new(super::super::MemoryRefValue::first(
                super::super::new_memory_ref(super::super::MemoryValue::undef_typed(
                    &super::super::ArrayElementType::F64,
                    0,
                )),
            ))),
            Value::Range(RangeValue {
                start: 0.0,
                step: 1.0,
                stop: 0.0,
                is_float: false,
                element_type: RangeElementType::Default,
                is_step_range: false,
            }),
            Value::SliceAll,
            // Struct types
            Value::Struct(super::super::StructInstance {
                type_id: 0,
                struct_name: String::new().into(),
                values: vec![],
            }),
            Value::StructRef(0),
            Value::Rng(RngInstance::xoshiro(0)),
            // Tuple types
            Value::Tuple(TupleValue { elements: vec![] }),
            Value::NamedTuple(NamedTupleValue::new(vec![], vec![]).unwrap()),
            Value::Pairs(PairsValue::new(vec![], vec![]).unwrap()),
            new_ref(Value::Nothing),
            Value::Generator(Box::new(GeneratorValue {
                callable: GeneratorCallable::FunctionIndex(0),
                iter: Box::new(Value::Nothing),
                result_element_type: None,
            })),
            // Type/Module types
            Value::DataType(Box::new(crate::types::JuliaType::Any)),
            Value::RuntimeTypeVar(Box::new(RuntimeTypeVarValue {
                id: 0,
                name: "T".to_string(),
                lower_bound: crate::types::JuliaType::Bottom,
                upper_bound: crate::types::JuliaType::Any,
            })),
            Value::RuntimeTypeName(Box::new(RuntimeTypeNameValue {
                name: "Any".to_string(),
            })),
            Value::Module(Box::new(ModuleValue::new("test"))),
            // Callable types
            Value::Function(FunctionValue::new("test")),
            Value::Closure(ClosureValue::new("test", vec![])),
            Value::ComposedFunction(ComposedFunctionValue {
                outer: Box::new(Value::Function(FunctionValue::new("f"))),
                inner: Box::new(Value::Function(FunctionValue::new("g"))),
            }),
            // IO
            Value::IO(IOValue::buffer_ref()),
            // Macro system types
            Value::Symbol(SymbolValue::new("")),
            Value::Expr(ExprValue::from_head("call", vec![])),
            Value::QuoteNode(Box::new(Value::Nothing)),
            Value::LineNumberNode(LineNumberNodeValue::new(0, None)),
            Value::GlobalRef(GlobalRefValue::new("", SymbolValue::new(""))),
            // Regex types
            Value::Regex(Box::new(RegexValue::new("", "").unwrap())),
            Value::RegexMatch(Box::new(RegexMatchValue {
                match_str: String::new(),
                captures: vec![],
                offset: 1,
                offsets: vec![],
            })),
            // Enum type
            Value::Enum {
                type_name: String::new(),
                value: 0,
            },
            // Core.SimpleVector (svec) — appended last to preserve the
            // positional indices the assertions below rely on (Issue #4722).
            Value::SimpleVector(TupleValue { elements: vec![] }),
            // Flat static-array (Issue #7964 Phase 1).
            Value::StaticArray(Box::new(StaticRealValue::new_vector(
                "SVector{2, Float64}",
                crate::vm::value::StaticElem::F64(vec![1.0, 2.0]),
            ))),
            // Zero-allocation inline static-array (Issue #7964 Phase 3).
            Value::StaticArrayInline(
                crate::vm::value::static_real::StaticArrayInlineData::try_from_elem(
                    2,
                    1,
                    &crate::vm::value::StaticElem::F64(vec![1.0, 2.0]),
                )
                .unwrap(),
            ),
        ];

        // Exhaustive match: if a new Value variant is added and not listed above,
        // this match will fail to compile with "non-exhaustive patterns" error.
        for v in &all_values {
            match v {
                Value::I8(_)
                | Value::I16(_)
                | Value::I32(_)
                | Value::I64(_)
                | Value::I128(_)
                | Value::BigInt(_)
                | Value::U8(_)
                | Value::U16(_)
                | Value::U32(_)
                | Value::U64(_)
                | Value::U128(_)
                | Value::Bool(_)
                | Value::F16(_)
                | Value::F32(_)
                | Value::F64(_)
                | Value::BigFloat(_)
                | Value::Str(_)
                | Value::Char(_)
                | Value::Nothing
                | Value::Missing
                | Value::Undef
                | Value::ExprArgs(_)
                | Value::Memory(_)
                | Value::MemoryRef(_)
                | Value::Range(_)
                | Value::SliceAll
                | Value::Struct(_)
                | Value::StructRef(_)
                | Value::Rng(_)
                | Value::Tuple(_)
                | Value::NamedTuple(_)
                | Value::Pairs(_)
                | Value::Ref(_)
                | Value::Generator(_)
                | Value::DataType(_)
                | Value::RuntimeTypeVar(_)
                | Value::RuntimeTypeName(_)
                | Value::Module(_)
                | Value::Function(_)
                | Value::Closure(_)
                | Value::ComposedFunction(_)
                | Value::IO(_)
                | Value::Symbol(_)
                | Value::Expr(_)
                | Value::QuoteNode(_)
                | Value::LineNumberNode(_)
                | Value::GlobalRef(_)
                | Value::Regex(_)
                | Value::RegexMatch(_)
                | Value::SimpleVector(_)
                | Value::Enum { .. }
                | Value::StaticArray(_)
                | Value::StaticArrayInline(_) => {}
            }
            // Verify Debug and runtime_type work for every variant
            let _ = format!("{:?}", v);
            let _ = v.runtime_type();
            let _ = v.value_type();
        }

        // Ensure we have at least as many test values as Value variants.
        // The exact count should match the number of variants in the Value enum.
        // 50 after removing the `Value::Dict` (Issue #6731) and `Value::Set`
        // (Issue #6732) carriers. 52 after adding `StaticArrayInline` (Issue #7964 Phase 3).
        // 53 after adding `RuntimeTypeName` for DataType.name (Issue #8451).
        assert_eq!(
            all_values.len(),
            53,
            "Expected 53 Value variants but found {}. \
             If you added a new Value variant, update this test and increment the count.",
            all_values.len()
        );

        assert_eq!(
            all_values[21].runtime_type(),
            crate::types::JuliaType::VectorOf(Box::new(crate::types::JuliaType::TupleOf(vec![
                crate::types::JuliaType::Struct("Complex{Float64}".to_string())
            ])))
        );
    }

    /// Verify that boxing large variants keeps Value enum bounded (Issue #3352/#4166/#5171).
    #[test]
    fn test_value_enum_size_is_compact() {
        // Issue #5171: `Value::Generator` carried a 104-byte `GeneratorValue`
        // inline (the single largest variant), forcing every stack op / slot copy
        // / Vec growth to move 112 bytes. Boxing it dropped the enum to 64 bytes.
        // Issue #7966 boxed `Regex`, #7977 boxed `DataType`, and #7976 shrank
        // `StructInstance` (`struct_name` String->Box<str>, 56->48 bytes).
        //
        // The enum is nonetheless still 64 bytes, and `struct_name` was NOT the
        // lever (empirically measured, Issue #7976): `Value` has alignment 16
        // (from the inline `I128(i128)`/`U128(u128)` variants), so its size must
        // be a multiple of 16. The largest variant payload is now 48 bytes
        // (`Struct`/`Pairs`/`NamedTuple`/`Function`), and 48 + the 1-byte tag
        // rounds up to 64 under 16-byte alignment. Dropping to 56 requires BOTH
        // boxing `I128`/`U128` (to make the enum 8-aligned) AND keeping every
        // payload <= 48; that alignment fix is tracked separately as the real
        // ceiling lever. Keep this bound tight so any new large variant fails the
        // audit.
        const MAX_TRANSITIONAL_VALUE_SIZE_BYTES: usize = 64;
        let size = std::mem::size_of::<Value>();
        assert!(
            size <= MAX_TRANSITIONAL_VALUE_SIZE_BYTES,
            "Value enum is {} bytes, expected at most {} (Issue #5171 boxed Generator down to 64). \
             Large variants should be boxed or moved behind registry handles to keep the enum bounded.",
            size,
            MAX_TRANSITIONAL_VALUE_SIZE_BYTES
        );
    }
}

/// ValueType represents the runtime type tags for Julia values in the VM.
///
/// This enum is used for:
/// - Method dispatch (matching argument types to method signatures)
/// - Type inference results (what type a variable or expression has)
/// - Code generation (selecting appropriate VM instructions)
///
/// # Adding New Variants
///
/// When adding a new variant to this enum, you MUST update ALL of the following files
/// to handle the new variant (the compiler will help catch some but not all):
///
/// 1. `compile/bridge.rs`:
///    - `impl From<&ValueType> for LatticeType` (ValueType → LatticeType conversion)
///    - `impl From<&LatticeType> for ValueType` (LatticeType → ValueType conversion)
///    - Add the variant to `test_all_valuetype_variants_to_lattice` test
///
/// 2. `compile/stmt.rs`:
///    - `emit_return_for_type` function (Return instructions)
///    - Store instruction match in `Stmt::While` handling
///    - Return instruction match in `Stmt::Return` handling
///
/// 3. `compile/expr/infer.rs`:
///    - `infer_literal_type` function (if applicable)
///    - `value_type_to_julia_type` function
///
/// 4. `compile/expr/mod.rs`:
///    - Type conversion rules in `compile_expr_as`
///
/// 5. `vm/builtins_reflection.rs`:
///    - `value_type_to_julia_type` function
///
/// 6. `bin/sjulia.rs` (Issue #1736):
///    - `format_value_with_vm()` function (REPL result formatting)
///    - Match arms in `run_file()` and `run_code()` return value handling
///
/// 7. `vm/formatting.rs` (Issue #1736):
///    - `format_value()` function (Value display formatting)
///    - `value_to_string()` function (Value-to-string conversion)
///
/// 8. `ffi/format.rs` (Issue #1736):
///    - `format_value()` function (FFI value formatting)
///
/// 9. `ffi/basic.rs` (Issue #1736):
///    - All functions returning or formatting Value results
///
/// # Quick Check Command
///
/// After adding a new variant, run:
/// ```bash
/// cargo build 2>&1 | grep "non-exhaustive patterns"
/// ```
///
/// # Related Types
///
/// - `LatticeType` in `compile/lattice/types.rs`: Compile-time abstract type (more precise)
/// - `JuliaType` in `types.rs`: Julia type representation for typeof() results
/// - `Value` in this file: The actual runtime value (ValueType is just the type tag)
///
/// Note: Copy removed because ArrayOf contains ArrayElementType which can contain Vec
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ValueType {
    // Signed integers
    I8,
    I16,
    I32,
    I64,
    I128,
    BigInt, // Arbitrary precision integer
    // Unsigned integers
    U8,
    U16,
    U32,
    U64,
    U128,
    // Boolean
    Bool,
    // Floating point
    F16,
    F32,
    F64,
    BigFloat, // Arbitrary precision float
    // Collections
    Array, // Legacy array type (treated as F64 for backward compatibility)
    ArrayOf(ArrayElementType, Option<usize>), // Array with known element type; 2nd field = rank (ndims), None = unknown→Vector (Issue #6817)
    Memory,                                   // Memory{T} flat typed buffer (element type unknown)
    MemoryOf(ArrayElementType),               // Memory{T} with known element type
    Range,                                    // Lazy range type
    // String types
    Str,
    Char, // Julia's Char type (32-bit Unicode codepoint)
    // Special types
    Nothing,       // Julia's Nothing type (type of `nothing`)
    Missing,       // Julia's Missing type (type of `missing`)
    Struct(usize), // type_id - includes Complex which is now a Pure Julia struct
    Rng,           // RNG instance type
    Tuple,         // Tuple type
    NamedTuple,    // Named tuple type
    Pairs,         // Base.Pairs type (for kwargs...)
    Dict,          // Dictionary type
    Set,           // Set type (unique elements)
    Generator,     // Generator type (lazy map)
    DataType,      // Type of types (returned by typeof)
    Module,        // Module type (e.g., Statistics, Base)
    Function,      // Function type (abstract supertype of all functions)
    IO,            // IO stream type
    // Macro system types
    Symbol,         // Julia Symbol type
    Expr,           // Julia Expr type
    QuoteNode,      // QuoteNode type
    LineNumberNode, // LineNumberNode type
    GlobalRef,      // GlobalRef type
    // Regex types
    Regex,      // Julia Regex type
    RegexMatch, // Julia RegexMatch type
    Any,        // Dynamic type - determined at runtime
    // Union type (preserves type information for optimization)
    Union(Vec<ValueType>), // Union of multiple types, e.g., Union{Int64, Float64}
    // Enum type (from @enum macro)
    Enum, // Enum type
    // Concrete Complex scalar tags used by runtime specialization. The runtime
    // value remains a Pure Julia StructRef/Struct; this avoids losing the
    // Complex element type in erased call paths.
    ComplexF32,
    ComplexF64,
}

pub(crate) fn value_type_for_struct_instance(s: &StructInstance) -> ValueType {
    match &*s.struct_name {
        "Complex{Float32}" | "ComplexF32" => ValueType::ComplexF32,
        "Complex{Float64}" | "ComplexF64" => ValueType::ComplexF64,
        _ => ValueType::Struct(s.type_id),
    }
}
