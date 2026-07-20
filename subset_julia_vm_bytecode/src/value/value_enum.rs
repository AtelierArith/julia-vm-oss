//! Value - The main runtime value type for the Julia VM.
//!
//! This module contains:
//! - `Value`: The main enum representing all Julia values at runtime
//! - runtime conversion from `Value` to the bytecode-owned `ValueType` tag

use crate::rng::RngInstance;
use crate::ValueType;
use half::f16;
use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

use super::array_element::{array_element_type_to_julia_type, julia_array_type_for_ndims};
use super::composed_function::ComposedFunctionValue;
use super::expr::ExprValue;
use super::generator::GeneratorValue;
use super::io::IORef;
use super::macro_::{BindingValue, GlobalRefValue, LineNumberNodeValue, SymbolValue};
use super::memory_value::{MemoryRef, MemoryRefValue};
use super::metadata::{ClosureValue, FunctionValue, ModuleValue};
use super::named_tuple::NamedTupleValue;
use super::pairs::PairsValue;
use super::range::RangeValue;
use super::regex::{RegexMatchValue, RegexValue};
use super::static_real::StaticRealValue;
use super::struct_instance::{is_complex_type_name, StructInstance};
use super::tuple::TupleValue;
use super::{ExprArgsCarrier, RustBigFloat, RustBigInt, BIGFLOAT_PRECISION};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeTypeVarValue {
    pub id: u64,
    pub name: String,
    pub lower_bound: subset_julia_vm_types::types::JuliaType,
    pub upper_bound: subset_julia_vm_types::types::JuliaType,
}

impl RuntimeTypeVarValue {
    pub fn projection(&self) -> subset_julia_vm_types::types::JuliaType {
        use subset_julia_vm_types::types::JuliaType;

        JuliaType::RuntimeTypeVar {
            id: self.id,
            name: self.name.clone(),
            lower_bound: Box::new(self.lower_bound.clone()),
            upper_bound: Box::new(self.upper_bound.clone()),
        }
    }

    pub fn source_anonymous_projection(&self) -> Option<subset_julia_vm_types::types::JuliaType> {
        use subset_julia_vm_types::types::{JuliaType, SOURCE_ANONYMOUS_TYPEVAR_NAME};

        if self.name != SOURCE_ANONYMOUS_TYPEVAR_NAME {
            return None;
        }
        let bound = match (&self.lower_bound, &self.upper_bound) {
            (JuliaType::Bottom, JuliaType::Any) => None,
            (JuliaType::Bottom, upper) => Some(upper.name().into_owned()),
            (lower, JuliaType::Any) => Some(format!(">:{}", lower.name())),
            (lower, upper) => Some(format!("{}<:_<:{}", lower.name(), upper.name())),
        };
        Some(JuliaType::TypeVar("_".to_string(), bound))
    }
}

#[cfg(test)]
mod runtime_typevar_projection_tests {
    use super::RuntimeTypeVarValue;
    use subset_julia_vm_types::types::JuliaType;

    #[test]
    fn anonymous_lower_bound_survives_projection() {
        let typevar = RuntimeTypeVarValue {
            id: 1,
            name: "_".to_string(),
            lower_bound: JuliaType::Int64,
            upper_bound: JuliaType::Any,
        };

        assert_eq!(
            typevar.projection(),
            JuliaType::RuntimeTypeVar {
                id: 1,
                name: "_".to_string(),
                lower_bound: Box::new(JuliaType::Int64),
                upper_bound: Box::new(JuliaType::Any),
            }
        );
    }

    #[test]
    fn anonymous_upper_bound_survives_projection() {
        let typevar = RuntimeTypeVarValue {
            id: 2,
            name: "_".to_string(),
            lower_bound: JuliaType::Bottom,
            upper_bound: JuliaType::Real,
        };

        assert_eq!(
            typevar.projection(),
            JuliaType::RuntimeTypeVar {
                id: 2,
                name: "_".to_string(),
                lower_bound: Box::new(JuliaType::Bottom),
                upper_bound: Box::new(JuliaType::Real),
            }
        );
    }

    #[test]
    fn anonymous_both_bounds_survive_projection() {
        let typevar = RuntimeTypeVarValue {
            id: 3,
            name: "_".to_string(),
            lower_bound: JuliaType::Int64,
            upper_bound: JuliaType::Real,
        };

        assert_eq!(
            typevar.projection(),
            JuliaType::RuntimeTypeVar {
                id: 3,
                name: "_".to_string(),
                lower_bound: Box::new(JuliaType::Int64),
                upper_bound: Box::new(JuliaType::Real),
            }
        );
    }

    #[test]
    fn named_both_bounds_survive_projection() {
        let typevar = RuntimeTypeVarValue {
            id: 4,
            name: "T".to_string(),
            lower_bound: JuliaType::Int64,
            upper_bound: JuliaType::Real,
        };

        assert_eq!(
            typevar.projection(),
            JuliaType::RuntimeTypeVar {
                id: 4,
                name: "T".to_string(),
                lower_bound: Box::new(JuliaType::Int64),
                upper_bound: Box::new(JuliaType::Real),
            }
        );
    }

    #[test]
    fn source_anonymous_marker_projects_to_legacy_existential() {
        let typevar = RuntimeTypeVarValue {
            id: 5,
            name: subset_julia_vm_types::types::SOURCE_ANONYMOUS_TYPEVAR_NAME.to_string(),
            lower_bound: JuliaType::Int64,
            upper_bound: JuliaType::Any,
        };

        assert_eq!(
            typevar.source_anonymous_projection(),
            Some(JuliaType::TypeVar(
                "_".to_string(),
                Some(">:Int64".to_string())
            ))
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeTypeNameValue {
    /// The leaf symbol exposed through `TypeName.name` (`A.T` -> `T`).
    pub name: String,
    /// Stable owner-qualified family key used for `TypeName` object identity.
    ///
    /// Upstream `jl_typename_t` stores `name` and `module` separately.  sjulia
    /// keeps the same distinction in a compact rendered key so sibling module
    /// declarations do not collapse merely because their leaf symbols match
    /// (Issue #8451).
    pub identity: String,
}

/// Interior-mutable reference cell backing `Base.RefValue{T}` (Issue #5130).
///
/// `Rc<RefCell<Value>>` gives `Ref` proper reference semantics: `r[] = v`
/// mutates the boxed value in place, and aliases observe the update — matching
/// upstream Julia's mutable single-element `RefValue` container. It also keeps
/// serving as the broadcast scalar wrapper (`isa(x, Ref)` is true).
pub type RefCellRef = Rc<RefCell<Value>>;
pub type WeakRefCell = Rc<RefCell<Value>>;

/// Construct a fresh `Ref` value wrapping `inner` (Issue #5130).
#[inline]
pub fn new_ref(inner: Value) -> Value {
    Value::Ref(Rc::new(RefCell::new(inner)))
}

/// Construct a fresh `WeakRef` value whose visible `.value` cell is maintained by
/// the VM weak-reference registry.
#[inline]
pub fn new_weak_ref(inner: Value) -> Value {
    Value::WeakRef(Rc::new(RefCell::new(inner)))
}

/// Shared, immutable string payload backing `Value::Str` (Issue #8630).
///
/// Julia's `String` is an immutable heap object; value semantics only ever
/// share it. `Rc<str>` mirrors that exactly: cloning a `Value::Str` bumps a
/// reference count instead of deep-copying the string body (the previous
/// `String` payload paid an `O(len)` heap allocation on every stack push /
/// slot store / container insert). The VM is single-threaded (design
/// principle 9 / `docs/vm/SINGLE_THREADED_VM.md`), so `Rc` — not `Arc` — is
/// the right choice. Payload width drops 24 → 16 bytes.
pub type StrRef = Rc<str>;
pub type StrBytesRef = Rc<[u8]>;

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
    // Shared immutable string (Issue #8630): `Rc<str>` so `Value` clone bumps a
    // refcount instead of deep-copying the body. Construct via `Value::str_new`.
    Str(StrRef),
    // Shared immutable Julia String bytes that are not valid UTF-8 (Issue
    // #8995). Valid UTF-8 strings keep using `Str`; this variant exists only
    // for upstream-compatible raw-byte String payloads such as
    // `String(UInt8[0xff])`.
    StrBytes(StrBytesRef),
    Char(char), // Julia's Char type (32-bit Unicode codepoint)
    // Malformed Julia `Char` (Issue #8995): the 32-bit Julia char bit pattern
    // (UTF-8-ish bytes left-aligned) of a byte sequence that is not a valid
    // Unicode scalar — produced by iterating/indexing a String with invalid
    // UTF-8 (e.g. `String(UInt8[0xff])[1] == '\xff'`) and by `'\xff'`-style
    // char literals. `typeof` is `Char`; `isvalid(c)` is false. Valid scalars
    // always use `Char` — construct through `Value::char_from_bits` so the
    // two variants stay disjoint.
    CharMalformed(u32),
    Nothing, // Julia's `nothing` value (singleton of type Nothing)
    Missing, // Julia's `missing` value (singleton of type Missing)
    Undef,   // Julia's #undef - uninitialized field value
    // Reference-counted (`Rc<RefCell<ArrayValue>>`) mutable array carrier. Its
    // sole runtime origin is `expr.args` — the mutable `Vector{Any}` AST args of
    // an `Expr`, which need auto-freed reference semantics (`struct_heap` has no
    // per-value GC, so a heap-`StructRef` wrapper would leak one slot per
    // transient `Expr` node). All *general* array values are the MemoryRef-backed
    // pure-Julia `Array{T,N}` wrapper; this carrier is confined to the `expr.args`
    // representation (and the generic array ops a `Vector{Any}` flows through).
    // Accessed via the `native_array_*` carrier helpers in `value/array_value`.
    // (Renamed from `NativeArray`, Issue #6807.) The payload is the private-field
    // witness newtype `ExprArgsCarrier` (Issue #8918): the carrier can only be
    // constructed / destructured through the `native_array_*` hub, so an
    // off-hub carrier site is a compile error (no grep allowlist needed).
    ExprArgs(ExprArgsCarrier),
    Memory(MemoryRef),              // Flat typed memory buffer (Memory{T})
    MemoryRef(Box<MemoryRefValue>), // Offset reference into Memory{T} (MemoryRef{T})
    Range(RangeValue),              // Lazy range (start:step:stop)
    SliceAll,                       // ':' slice marker for indexing
    Struct(StructInstance),         // User-defined struct (immutable), also Complex numbers
    StructRef(usize),               // Mutable struct reference (heap index)
    Rng(RngInstance),               // RNG instance (StableRNG/Xoshiro/MersenneTwister)
    Tuple(TupleValue),              // Immutable tuple
    SimpleVector(TupleValue), // Core.SimpleVector (svec) - returned by <DataType>.parameters (Issue #4722)
    NamedTuple(NamedTupleValue), // Named tuple
    Pairs(PairsValue),        // Base.Pairs (for kwargs...)
    Ref(RefCellRef), // Base.RefValue{T} - mutable single-element box (Issue #5130); also protects value from broadcasting (treated as scalar)
    WeakRef(WeakRefCell), // Base.WeakRef - weak cell whose target is cleared by VM GC
    Generator(Box<GeneratorValue>), // Lazy generator (boxed: 104->8 bytes, Issue #5171)
    DataType(Box<subset_julia_vm_types::types::JuliaType>), // DataType (boxed: 56->8 bytes, Issue #7977/#7966)
    RuntimeTypeVar(Box<RuntimeTypeVarValue>),               // Fresh TypeVar object with identity
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
    Binding(Box<BindingValue>), // Core.Binding - global binding metadata
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
    StaticArrayInline(crate::value::static_real::StaticArrayInlineData),
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
    StrBytes(Vec<u8>),
    CharMalformed(u32),
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
            // Wire format unchanged: serialize the string body itself, so
            // caches written before/after the Rc<str> migration are compatible
            // (Issue #8630/#8631).
            Value::Str(v) => SerializableValue::Str(v.to_string()),
            Value::StrBytes(v) => SerializableValue::StrBytes(v.to_vec()),
            Value::Char(v) => SerializableValue::Char(*v),
            Value::CharMalformed(v) => SerializableValue::CharMalformed(*v),
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
            SerializableValue::Str(v) => Value::str_new(v),
            SerializableValue::Char(v) => Value::Char(v),
            SerializableValue::Symbol(v) => Value::Symbol(SymbolValue::new(v)),
            SerializableValue::Enum { type_name, value } => Value::Enum { type_name, value },
            SerializableValue::StrBytes(v) => Value::str_from_bytes(v),
            SerializableValue::CharMalformed(v) => Value::CharMalformed(v),
        })
    }
}

impl Value {
    /// Reify a structured type projection as its runtime type-object value.
    /// Runtime TypeVars retain their object id instead of being boxed as a
    /// `DataType`, which is required for `.lb`/`.ub` reflection identity.
    pub fn type_object(ty: subset_julia_vm_types::types::JuliaType) -> Value {
        use subset_julia_vm_types::types::JuliaType;
        match ty {
            JuliaType::RuntimeTypeVar {
                id,
                name,
                lower_bound,
                upper_bound,
            } => Value::RuntimeTypeVar(Box::new(RuntimeTypeVarValue {
                id,
                name,
                lower_bound: *lower_bound,
                upper_bound: *upper_bound,
            })),
            other => Value::DataType(Box::new(other)),
        }
    }

    /// Construct a `Value::Str` from anything convertible into a shared
    /// immutable string (Issue #8630). This is the single hub for string-value
    /// construction: `&str`, `String`, `Box<str>`, and `Rc<str>` all flow
    /// through `Into<Rc<str>>`. Passing an owned `String` moves its buffer into
    /// the `Rc` (one allocation, no copy); passing a `&str` copies once into a
    /// fresh `Rc`. Downstream clones of the resulting `Value` only bump the
    /// refcount.
    #[inline]
    pub fn str_new(s: impl Into<StrRef>) -> Value {
        Value::Str(s.into())
    }

    /// Construct a Julia Char from a Julia 32-bit char pattern (Issue #8995):
    /// the fast `char` variant when the pattern is a well-formed encoding of
    /// a Unicode scalar, `CharMalformed` otherwise. This is the only
    /// constructor for `CharMalformed`, keeping the two variants disjoint.
    #[inline]
    pub fn char_from_bits(bits: u32) -> Value {
        match crate::value::julia_char::julia_char_from_bits(bits) {
            Some(c) => Value::Char(c),
            None => Value::CharMalformed(bits),
        }
    }

    /// Construct a Julia String from raw bytes. Julia `String` can carry invalid
    /// UTF-8; keep the fast `Rc<str>` representation for valid UTF-8 and use a
    /// byte-backed payload only for invalid input (Issue #8995).
    #[inline]
    pub fn str_from_bytes(bytes: Vec<u8>) -> Value {
        match String::from_utf8(bytes) {
            Ok(s) => Value::str_new(s),
            Err(err) => Value::StrBytes(Rc::from(err.into_bytes())),
        }
    }

    #[inline]
    pub fn string_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Str(s) => Some(s.as_bytes()),
            Value::StrBytes(bytes) => Some(bytes.as_ref()),
            _ => None,
        }
    }

    #[inline]
    pub fn string_lossy(&self) -> Option<Cow<'_, str>> {
        match self {
            Value::Str(s) => Some(Cow::Borrowed(s.as_ref())),
            Value::StrBytes(bytes) => Some(String::from_utf8_lossy(bytes.as_ref())),
            _ => None,
        }
    }

    /// Get the runtime type of this value as a JuliaType.
    #[inline]
    pub fn runtime_type(&self) -> subset_julia_vm_types::types::JuliaType {
        // Route the legacy native-array carrier through the shared
        // `super::array_value::native_array_value_ref` helper so the match
        // below no longer holds a native-array arm (Issue #3908).
        if let Some(arr_ref) = super::array_value::native_array_value_ref(self) {
            let arr = arr_ref.borrow();
            if let Some(container_type) = arr.array_type_override() {
                return subset_julia_vm_types::types::JuliaType::Struct(container_type.to_string());
            }
            let elem_type = array_element_type_to_julia_type(&arr.element_type());
            return julia_array_type_for_ndims(elem_type, arr.shape.len());
        }
        match self {
            // Signed integers
            Value::I8(_) => subset_julia_vm_types::types::JuliaType::Int8,
            Value::I16(_) => subset_julia_vm_types::types::JuliaType::Int16,
            Value::I32(_) => subset_julia_vm_types::types::JuliaType::Int32,
            Value::I64(_) => subset_julia_vm_types::types::JuliaType::Int64,
            Value::I128(_) => subset_julia_vm_types::types::JuliaType::Int128,
            Value::BigInt(_) => subset_julia_vm_types::types::JuliaType::BigInt,
            // Unsigned integers
            Value::U8(_) => subset_julia_vm_types::types::JuliaType::UInt8,
            Value::U16(_) => subset_julia_vm_types::types::JuliaType::UInt16,
            Value::U32(_) => subset_julia_vm_types::types::JuliaType::UInt32,
            Value::U64(_) => subset_julia_vm_types::types::JuliaType::UInt64,
            Value::U128(_) => subset_julia_vm_types::types::JuliaType::UInt128,
            // Boolean
            Value::Bool(_) => subset_julia_vm_types::types::JuliaType::Bool,
            // Floating point
            Value::F16(_) => subset_julia_vm_types::types::JuliaType::Float16,
            Value::F32(_) => subset_julia_vm_types::types::JuliaType::Float32,
            Value::F64(_) => subset_julia_vm_types::types::JuliaType::Float64,
            Value::BigFloat(_) => subset_julia_vm_types::types::JuliaType::BigFloat,
            Value::Str(_) | Value::StrBytes(_) => subset_julia_vm_types::types::JuliaType::String,
            Value::Char(_) | Value::CharMalformed(_) => {
                subset_julia_vm_types::types::JuliaType::Char
            }
            Value::Nothing => subset_julia_vm_types::types::JuliaType::Nothing,
            Value::Missing => subset_julia_vm_types::types::JuliaType::Missing,
            Value::Undef => subset_julia_vm_types::types::JuliaType::Any, // #undef has no type
            Value::Memory(mem) => {
                let mem = mem.borrow();
                let elem_type_name = mem.element_type().julia_type_name();
                subset_julia_vm_types::types::JuliaType::Struct(format!(
                    "Memory{{{}}}",
                    elem_type_name
                ))
            }
            Value::MemoryRef(memref) => {
                subset_julia_vm_types::types::JuliaType::Struct(memref.julia_type_name())
            }
            Value::Range(r) => subset_julia_vm_types::types::JuliaType::Struct(r.julia_type_name()),
            Value::SliceAll => subset_julia_vm_types::types::JuliaType::Any,
            Value::Struct(s) => {
                // Complex numbers are now Pure Julia structs, not a primitive type
                if let Some(array_type) = s.array_wrapper_julia_type() {
                    array_type
                } else if s.struct_name.is_empty() {
                    subset_julia_vm_types::types::JuliaType::Any
                } else {
                    subset_julia_vm_types::types::JuliaType::Struct(s.struct_name.to_string())
                }
            }
            Value::StructRef(_) => subset_julia_vm_types::types::JuliaType::Any, // StructRef needs VM context to resolve
            // typeof(rng): report the concrete RNG type. The global handle
            // (default_rng()/GLOBAL_RNG) reports as TaskLocalRNG (Issues #7230, #7231).
            Value::Rng(rng) => subset_julia_vm_types::types::JuliaType::Struct(
                match rng {
                    RngInstance::Stable(_) => "StableRNG",
                    RngInstance::Xoshiro(_) => "Xoshiro",
                    RngInstance::Mersenne(_) => "MersenneTwister",
                    RngInstance::Global => "TaskLocalRNG",
                }
                .to_string(),
            ),
            Value::Tuple(t) => {
                let element_types: Vec<subset_julia_vm_types::types::JuliaType> =
                    t.elements.iter().map(|e| e.runtime_type()).collect();
                subset_julia_vm_types::types::JuliaType::TupleOf(element_types)
            }
            // Issue #4722: typeof(<DataType>.parameters) === Core.SimpleVector.
            Value::SimpleVector(_) => {
                subset_julia_vm_types::types::JuliaType::Struct("Core.SimpleVector".to_string())
            }
            Value::NamedTuple(_) => subset_julia_vm_types::types::JuliaType::NamedTuple,
            Value::Ref(inner) => {
                // Base.RefValue{T}: typeof(Ref(5)) === Base.RefValue{Int64} (Issue #5130)
                let inner_ty = inner.borrow().runtime_type();
                subset_julia_vm_types::types::JuliaType::Struct(format!(
                    "Base.RefValue{{{}}}",
                    inner_ty
                ))
            }
            Value::WeakRef(_) => {
                subset_julia_vm_types::types::JuliaType::Struct("WeakRef".to_string())
            }
            Value::Generator(_) => subset_julia_vm_types::types::JuliaType::Generator, // Generator type
            Value::DataType(_) => subset_julia_vm_types::types::JuliaType::DataType, // typeof(typeof(x)) == DataType
            Value::RuntimeTypeVar(_) => {
                subset_julia_vm_types::types::JuliaType::Struct("TypeVar".to_string())
            }
            Value::RuntimeTypeName(_) => {
                subset_julia_vm_types::types::JuliaType::Struct("Core.TypeName".to_string())
            }
            Value::Module(_) => subset_julia_vm_types::types::JuliaType::Module, // typeof(Statistics) == Module
            Value::Function(f) => {
                subset_julia_vm_types::types::JuliaType::Struct(f.singleton_type_name())
            } // callable singleton <: Function
            // Each closure definition site is its own callable singleton type
            // `typeof(<qualified nested name>)`, mirroring the named-function
            // arm above (Issue #9106). Reporting the shared `Function` type
            // here made `typeof(closure)` collapse to `Function` and broke
            // `::typeof(f)` dispatch on closure-valued bindings.
            Value::Closure(cv) => {
                subset_julia_vm_types::types::JuliaType::Struct(cv.singleton_type_name())
            }
            Value::ComposedFunction(_) => subset_julia_vm_types::types::JuliaType::Function, // Composed functions are also Functions
            Value::IO(io_ref) => {
                if io_ref.borrow().is_pipe() {
                    subset_julia_vm_types::types::JuliaType::Struct("Pipe".to_string())
                } else {
                    subset_julia_vm_types::types::JuliaType::IOBuffer
                }
            }
            // Macro system types
            Value::Symbol(_) => subset_julia_vm_types::types::JuliaType::Symbol,
            Value::Expr(_) => subset_julia_vm_types::types::JuliaType::Expr,
            Value::QuoteNode(_) => subset_julia_vm_types::types::JuliaType::QuoteNode,
            Value::LineNumberNode(_) => subset_julia_vm_types::types::JuliaType::LineNumberNode,
            Value::GlobalRef(_) => subset_julia_vm_types::types::JuliaType::GlobalRef,
            Value::Binding(_) => {
                subset_julia_vm_types::types::JuliaType::Struct("Core.Binding".to_string())
            }
            Value::Pairs(_) => subset_julia_vm_types::types::JuliaType::Pairs,
            Value::Regex(_) => subset_julia_vm_types::types::JuliaType::Struct("Regex".to_string()),
            Value::RegexMatch(_) => {
                subset_julia_vm_types::types::JuliaType::Struct("RegexMatch".to_string())
            }
            Value::Enum { type_name, .. } => {
                subset_julia_vm_types::types::JuliaType::Enum(type_name.clone())
            }
            // Flat static-array representation (Issue #7964): reports the same
            // concrete Julia type as the Struct+Tuple representation it replaces.
            Value::StaticArray(sv) => {
                subset_julia_vm_types::types::JuliaType::Struct(sv.julia_type_name().to_string())
            }
            Value::StaticArrayInline(sv) => subset_julia_vm_types::types::JuliaType::Struct(
                sv.julia_type_name_owned().to_string(),
            ),
            // The legacy native-array carrier is filtered out by the
            // early-return above (Issue #3908). This wildcard satisfies
            // Rust's exhaustiveness checking and provides a safe default for
            // any future `Value` variant: return `Any`.
            _ => subset_julia_vm_types::types::JuliaType::Any,
        }
    }

    /// Get the ValueType of this value.
    #[inline]
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
            Value::Str(_) | Value::StrBytes(_) => ValueType::Str,
            Value::Char(_) | Value::CharMalformed(_) => ValueType::Char,
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
            Value::WeakRef(_) => ValueType::Any,
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
            Value::Binding(_) => ValueType::Any,
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
    #[inline]
    pub fn is_complex(&self) -> bool {
        match self {
            Value::Struct(s) => s.is_complex(),
            _ => false,
        }
    }

    /// Extract (re, im) from a complex value (Complex struct)
    /// Returns None if not a complex value
    /// Note: Also returns Some for I64/F64 values (promoted to complex with im=0)
    #[inline]
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
    #[inline]
    pub fn is_bigint(&self) -> bool {
        matches!(self, Value::BigInt(_))
    }

    /// Get the BigInt value if this is a BigInt, otherwise None
    #[inline]
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
    #[inline]
    pub fn is_bigfloat(&self) -> bool {
        matches!(self, Value::BigFloat(_))
    }

    /// Get the BigFloat value if this is a BigFloat, otherwise None
    #[inline]
    pub fn as_bigfloat(&self) -> Option<&RustBigFloat> {
        match self {
            Value::BigFloat(v) => Some(v),
            _ => None,
        }
    }
}

/// Classification of a `Core.Binding` field access, distinguishing three
/// upstream-observable outcomes (Issue #10067):
///
/// - a modeled field with a concrete runtime value (`:globalref`, `:flags`);
/// - a field that exists in upstream's `Core.Binding` fieldnames
///   (`:globalref, :value, :partitions, :backedges, :flags`) but sjulia does
///   not track a runtime value for it — upstream throws `UndefRefError`
///   ("access to undefined reference") for these, NOT a missing-field error;
/// - a name that is not one of `Core.Binding`'s fieldnames at all — upstream
///   throws `FieldError`.
///
/// Centralizing this table lets `getfield`-by-name, `getfield`-by-index, and
/// `isdefined` share one source of truth instead of three independently
/// maintained match arms that can silently drift apart.
#[derive(Debug, Clone)]
pub enum BindingFieldAccess {
    /// The field is modeled and has a concrete value.
    Value(Value),
    /// The field is part of upstream's `Core.Binding` layout but is unset /
    /// unmodeled in sjulia.
    Undef,
    /// The field name/index is not part of `Core.Binding` at all.
    NoField,
}

/// Upstream `fieldnames(Core.Binding)`, in declaration order (Issue #10067).
pub const BINDING_FIELD_NAMES: [&str; 5] =
    ["globalref", "value", "partitions", "backedges", "flags"];

impl BindingValue {
    /// Classify a `getfield`/`getproperty` access to a `Core.Binding` field
    /// by name. See [`BindingFieldAccess`] for the three possible outcomes.
    pub fn field_by_name(&self, field_name: &str) -> BindingFieldAccess {
        match field_name {
            "globalref" => BindingFieldAccess::Value(Value::GlobalRef(self.global_ref.clone())),
            "flags" => BindingFieldAccess::Value(Value::U8(self.flags)),
            "value" | "partitions" | "backedges" => BindingFieldAccess::Undef,
            _ => BindingFieldAccess::NoField,
        }
    }

    /// Classify a `getfield`/`getproperty` access to a `Core.Binding` field
    /// by 0-based positional index (upstream `getfield(b, i)` is 1-based;
    /// callers subtract 1 before calling this). See [`BindingFieldAccess`].
    pub fn field_by_index(&self, field_idx: usize) -> BindingFieldAccess {
        match field_idx {
            0 => BindingFieldAccess::Value(Value::GlobalRef(self.global_ref.clone())),
            4 => BindingFieldAccess::Value(Value::U8(self.flags)),
            1..=3 => BindingFieldAccess::Undef,
            _ => BindingFieldAccess::NoField,
        }
    }

    /// Whether `field_name` is set (mirrors upstream `isdefined(b, s)`).
    pub fn is_field_defined(&self, field_name: &str) -> bool {
        matches!(self.field_by_name(field_name), BindingFieldAccess::Value(_))
    }

    /// Whether the field at 0-based `field_idx` is set (mirrors upstream
    /// `isdefined(b, i)`; upstream's out-of-range index returns `false`
    /// rather than erroring, matching [`BindingFieldAccess::NoField`] here).
    pub fn is_field_defined_by_index(&self, field_idx: usize) -> bool {
        matches!(self.field_by_index(field_idx), BindingFieldAccess::Value(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::RngInstance;
    use crate::value::io::IOValue;
    use crate::value::ArrayElementType;
    use crate::value::GeneratorCallable;

    /// Cache/wire compatibility for `Value::Str` after the `Rc<str>` migration
    /// (Issue #8631, parent #8612). `Value` serializes through
    /// `SerializableValue::Str(String)`, so the on-disk bincode bytes are
    /// determined solely by the string content and are independent of whether
    /// the payload is a `String` or an `Rc<str>`. This pins the exact wire
    /// bytes so a future representation change that would silently break
    /// prelude/Base cache and `.sjvmbc` compatibility fails loudly.
    #[test]
    fn test_value_str_bincode_wire_is_stable_8631() {
        // Representation-independence: constructing the same string value via
        // `&str`, `String`, or a pre-existing `Rc<str>` must serialize to
        // identical bytes.
        let from_str = Value::str_new("cache-8631");
        let from_string = Value::str_new(String::from("cache-8631"));
        let shared: Rc<str> = Rc::from("cache-8631");
        let from_rc = Value::str_new(shared);
        let b_str = bincode::serialize(&from_str).expect("serialize &str-built Str");
        let b_string = bincode::serialize(&from_string).expect("serialize String-built Str");
        let b_rc = bincode::serialize(&from_rc).expect("serialize Rc-built Str");
        assert_eq!(b_str, b_string);
        assert_eq!(b_str, b_rc);

        // Exact wire bytes for a small ASCII string. `SerializableValue::Str`
        // is the 18th variant (index 17); bincode's default config encodes the
        // enum discriminant as a little-endian u32 and the `String` as a
        // little-endian u64 length followed by the UTF-8 body.
        let bytes = bincode::serialize(&Value::str_new("hi")).expect("serialize");
        assert_eq!(
            bytes,
            vec![17, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, b'h', b'i'],
            "Value::Str bincode wire format changed — this breaks prelude/Base \
             cache and .sjvmbc compatibility (Issue #8631)"
        );

        // Round-trip through bincode preserves content (including non-ASCII).
        for s in ["", "hi", "héllo", "π ≈ 3.14", "tab\tnewline\n"] {
            let v = Value::str_new(s);
            let encoded = bincode::serialize(&v).expect("serialize");
            let decoded: Value = bincode::deserialize(&encoded).expect("deserialize");
            match decoded {
                Value::Str(ref got) => assert_eq!(got.as_ref(), s),
                other => panic!("round-trip produced non-Str: {:?}", other),
            }
        }

        let invalid = Value::str_from_bytes(vec![0xff, b'a']);
        let encoded = bincode::serialize(&invalid).expect("serialize invalid bytes");
        let decoded: Value = bincode::deserialize(&encoded).expect("deserialize invalid bytes");
        match decoded {
            Value::StrBytes(ref got) => assert_eq!(got.as_ref(), [0xff, b'a']),
            other => panic!("invalid round-trip produced non-StrBytes: {:?}", other),
        }
    }

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
            Value::str_new(String::new()),
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
                element_type: super::super::RangeElementType::Default,
                step_type: super::super::RangeElementType::Default,
                is_step_range: false,
                linspace_len: None,
                step_defined: false,
                bigint: None,
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
            new_weak_ref(Value::Nothing),
            Value::Generator(Box::new(GeneratorValue {
                callable: GeneratorCallable::FunctionIndex(0),
                iter: Box::new(Value::Nothing),
                result_element_type: None,
            })),
            // Type/Module types
            Value::DataType(Box::new(subset_julia_vm_types::types::JuliaType::Any)),
            Value::RuntimeTypeVar(Box::new(RuntimeTypeVarValue {
                id: 0,
                name: "T".to_string(),
                lower_bound: subset_julia_vm_types::types::JuliaType::Bottom,
                upper_bound: subset_julia_vm_types::types::JuliaType::Any,
            })),
            Value::RuntimeTypeName(Box::new(RuntimeTypeNameValue {
                name: "Any".to_string(),
                identity: "Any".to_string(),
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
            Value::Binding(Box::new(BindingValue::new(GlobalRefValue::new(
                "",
                SymbolValue::new(""),
            )))),
            // Regex types
            Value::Regex(Box::new(RegexValue::new("", "").unwrap())),
            Value::RegexMatch(Box::new(RegexMatchValue {
                match_str: String::new(),
                captures: vec![],
                offset: 1,
                offsets: vec![],
                capture_names: vec![],
                regex: RegexValue::new("", "").unwrap(),
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
                crate::value::StaticElem::F64(vec![1.0, 2.0]),
            ))),
            // Zero-allocation inline static-array (Issue #7964 Phase 3).
            Value::StaticArrayInline(
                crate::value::static_real::StaticArrayInlineData::try_from_elem(
                    2,
                    1,
                    &crate::value::StaticElem::F64(vec![1.0, 2.0]),
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
                | Value::StrBytes(_)
                | Value::Char(_)
                | Value::CharMalformed(_)
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
                | Value::WeakRef(_)
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
                | Value::Binding(_)
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
        // 54 after adding `Binding` for GlobalRef.binding (Issue #10014).
        // 55 after adding `WeakRef` for Base.WeakRef (Issue #8990).
        assert_eq!(
            all_values.len(),
            55,
            "Expected 55 Value variants but found {}. \
             If you added a new Value variant, update this test and increment the count.",
            all_values.len()
        );

        assert_eq!(
            all_values[21].runtime_type(),
            subset_julia_vm_types::types::JuliaType::VectorOf(Box::new(
                subset_julia_vm_types::types::JuliaType::TupleOf(vec![
                    subset_julia_vm_types::types::JuliaType::Struct("Complex{Float64}".to_string())
                ])
            ))
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
        // Issue #8630 migrated `Str(String)` -> `Str(Rc<str>)`, shrinking that
        // payload 24->16 bytes (it is no longer near the ceiling) and making
        // `Value` clone of a string an `Rc` refcount bump instead of an
        // `O(len)` heap copy.
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

#[inline]
pub fn value_type_for_struct_instance(s: &StructInstance) -> ValueType {
    match &*s.struct_name {
        "Complex{Float32}" | "ComplexF32" => ValueType::ComplexF32,
        "Complex{Float64}" | "ComplexF64" => ValueType::ComplexF64,
        name if is_complex_type_name(name) => match s.values.as_slice() {
            [Value::F32(_), Value::F32(_)] => ValueType::ComplexF32,
            [Value::F64(_), Value::F64(_)] => ValueType::ComplexF64,
            _ => ValueType::Struct(s.type_id),
        },
        name if name == "Array" || name.starts_with("Array{") => {
            array_wrapper_value_type(s).unwrap_or(ValueType::Struct(s.type_id))
        }
        _ => ValueType::Struct(s.type_id),
    }
}

/// Map a pure-Julia `Array{T,N}` wrapper `StructInstance` (Issue #2760/#6648:
/// `ref::MemoryRef{T}` + `size::NTuple{N,Int}`) to the `ValueType::ArrayOf`
/// representation the runtime specializer (Issue #6346 `IndexAssign`) and the
/// legacy native-array carrier already use.
///
/// Before this (Issue #10566 blocker (a)), a modern MemoryRef-backed
/// `Vector{Int64}`/`Vector{Float64}` argument presented as `ValueType::Struct(id)`
/// to the runtime specializer, which only recognized `ValueType::ArrayOf` — so
/// `CallSpecialize` sites for functions taking such a Vector never produced a
/// typed body and silently fell back to the generic interpreter forever.
///
/// Returns `None` when the wrapper's fields don't match the expected shape
/// (e.g. mid-construction, or a variant this helper doesn't recognize), in
/// which case the caller falls back to the conservative `Struct(type_id)`.
#[inline]
fn array_wrapper_value_type(s: &StructInstance) -> Option<ValueType> {
    let ref_value = s.values.first()?;
    let elem_ty = array_wrapper_element_type(ref_value)?;
    let size_value = s.values.get(1)?;
    // Require a well-formed shape. A wrapper whose `size` field is not a tuple
    // is not a validated array (mid-construction, or a representation this
    // helper does not model); claiming `ArrayOf` for it would tell every
    // `ValueType::ArrayOf` consumer it is a real typed array on the strength of
    // the struct's *name* alone. Fall back to the conservative
    // `Struct(type_id)` instead — that is exactly the pre-#10566 behavior, so
    // the worst case is "not specialized", never a mis-typed body.
    let rank = array_wrapper_rank(size_value)?;
    Some(ValueType::ArrayOf(elem_ty, Some(rank)))
}

/// Extract the element type from an `Array{T,N}` wrapper's `ref` field, which
/// may be a `Memory{T}` (`Value::Memory`), a `MemoryRef{T}` (`Value::MemoryRef`),
/// or — for the transitional legacy carrier (Issue #6653/#6807) — the native
/// `ExprArgsCarrier` array value.
fn array_wrapper_element_type(ref_value: &Value) -> Option<crate::value::ArrayElementType> {
    match ref_value {
        Value::Memory(mem) => Some(mem.borrow().element_type().clone()),
        Value::MemoryRef(memref) => Some(memref.element_type()),
        other if super::array_value::is_native_array_value(other) => {
            super::array_value::native_array_value_ref(other).map(|arr| arr.borrow().element_type())
        }
        _ => None,
    }
}

/// Extract the rank (ndims) from an `Array{T,N}` wrapper's `size` field. The
/// field is normally `NTuple{N,Int}` (rank == tuple arity), but an internal
/// pop/shift optimization (Issue #6873, see `array_mutate.rs`) can temporarily
/// encode a 1-D vector's size as `((len,), offset)`; that shape's rank is the
/// inner tuple's arity, not the outer 2-element tuple's.
fn array_wrapper_rank(size_value: &Value) -> Option<usize> {
    let Value::Tuple(size_tuple) = size_value else {
        return None;
    };
    if let Some(Value::Tuple(inner)) = size_tuple.elements.first() {
        return Some(inner.elements.len());
    }
    Some(size_tuple.elements.len())
}
