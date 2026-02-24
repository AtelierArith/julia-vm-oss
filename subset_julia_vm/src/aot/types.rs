//! Type definitions for AoT compilation
//!
//! This module defines the type system used during AoT compilation,
//! including Julia type representations and type lattice operations.
//!
//! # StaticType vs JuliaType
//!
//! - `JuliaType`: General Julia type representation used throughout AoT compilation
//! - `StaticType`: Specifically for tracking statically-inferred types with Rust mappings
//!
//! StaticType is designed for code generation where we need to know exact Rust types.

use std::fmt;

/// Julia type representation for AoT compilation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JuliaType {
    /// Any type (top of lattice)
    Any,
    /// Bottom type (no value)
    Bottom,
    /// Nothing type
    Nothing,
    /// Missing type
    Missing,
    /// Boolean type
    Bool,
    /// Integer types
    Int8,
    Int16,
    Int32,
    Int64,
    Int128,
    /// Unsigned integer types
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    UInt128,
    /// Floating point types
    Float16,
    Float32,
    Float64,
    /// Character type
    Char,
    /// String type
    String,
    /// Symbol type
    Symbol,
    /// Array type with element type and dimensions
    Array {
        element_type: Box<JuliaType>,
        ndims: Option<usize>,
    },
    /// Tuple type
    Tuple(Vec<JuliaType>),
    /// Union type
    Union(Vec<JuliaType>),
    /// User-defined struct
    Struct {
        name: std::string::String,
        type_params: Vec<JuliaType>,
    },
    /// Type variable (for generics)
    TypeVar(std::string::String),
    /// Unknown type (needs inference)
    Unknown,
}

impl JuliaType {
    /// Check if this type is concrete (fully known)
    pub fn is_concrete(&self) -> bool {
        match self {
            JuliaType::Any | JuliaType::Unknown | JuliaType::TypeVar(_) => false,
            JuliaType::Union(types) => types.iter().all(|t| t.is_concrete()),
            JuliaType::Array { element_type, .. } => element_type.is_concrete(),
            JuliaType::Tuple(types) => types.iter().all(|t| t.is_concrete()),
            JuliaType::Struct { type_params, .. } => type_params.iter().all(|t| t.is_concrete()),
            _ => true,
        }
    }

    /// Check if this type is numeric
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            JuliaType::Int8
                | JuliaType::Int16
                | JuliaType::Int32
                | JuliaType::Int64
                | JuliaType::Int128
                | JuliaType::UInt8
                | JuliaType::UInt16
                | JuliaType::UInt32
                | JuliaType::UInt64
                | JuliaType::UInt128
                | JuliaType::Float16
                | JuliaType::Float32
                | JuliaType::Float64
        )
    }

    /// Check if this type is an integer type
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            JuliaType::Int8
                | JuliaType::Int16
                | JuliaType::Int32
                | JuliaType::Int64
                | JuliaType::Int128
                | JuliaType::UInt8
                | JuliaType::UInt16
                | JuliaType::UInt32
                | JuliaType::UInt64
                | JuliaType::UInt128
        )
    }

    /// Check if this type is a floating point type
    pub fn is_float(&self) -> bool {
        matches!(
            self,
            JuliaType::Float16 | JuliaType::Float32 | JuliaType::Float64
        )
    }

    /// Get the Rust type name for this Julia type
    pub fn to_rust_type(&self) -> std::string::String {
        match self {
            JuliaType::Bool => "bool".to_string(),
            JuliaType::Int8 => "i8".to_string(),
            JuliaType::Int16 => "i16".to_string(),
            JuliaType::Int32 => "i32".to_string(),
            JuliaType::Int64 => "i64".to_string(),
            JuliaType::Int128 => "i128".to_string(),
            JuliaType::UInt8 => "u8".to_string(),
            JuliaType::UInt16 => "u16".to_string(),
            JuliaType::UInt32 => "u32".to_string(),
            JuliaType::UInt64 => "u64".to_string(),
            JuliaType::UInt128 => "u128".to_string(),
            JuliaType::Float16 => "f32".to_string(), // No f16 in Rust
            JuliaType::Float32 => "f32".to_string(),
            JuliaType::Float64 => "f64".to_string(),
            JuliaType::Char => "char".to_string(),
            JuliaType::String => "String".to_string(),
            JuliaType::Nothing => "()".to_string(),
            JuliaType::Array { element_type, .. } => {
                format!("Vec<{}>", element_type.to_rust_type())
            }
            JuliaType::Tuple(types) => {
                let inner: Vec<_> = types.iter().map(|t| t.to_rust_type()).collect();
                format!("({})", inner.join(", "))
            }
            _ => "Value".to_string(), // Fallback to dynamic Value
        }
    }
}

impl fmt::Display for JuliaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JuliaType::Any => write!(f, "Any"),
            JuliaType::Bottom => write!(f, "Union{{}}"),
            JuliaType::Nothing => write!(f, "Nothing"),
            JuliaType::Missing => write!(f, "Missing"),
            JuliaType::Bool => write!(f, "Bool"),
            JuliaType::Int8 => write!(f, "Int8"),
            JuliaType::Int16 => write!(f, "Int16"),
            JuliaType::Int32 => write!(f, "Int32"),
            JuliaType::Int64 => write!(f, "Int64"),
            JuliaType::Int128 => write!(f, "Int128"),
            JuliaType::UInt8 => write!(f, "UInt8"),
            JuliaType::UInt16 => write!(f, "UInt16"),
            JuliaType::UInt32 => write!(f, "UInt32"),
            JuliaType::UInt64 => write!(f, "UInt64"),
            JuliaType::UInt128 => write!(f, "UInt128"),
            JuliaType::Float16 => write!(f, "Float16"),
            JuliaType::Float32 => write!(f, "Float32"),
            JuliaType::Float64 => write!(f, "Float64"),
            JuliaType::Char => write!(f, "Char"),
            JuliaType::String => write!(f, "String"),
            JuliaType::Symbol => write!(f, "Symbol"),
            JuliaType::Array {
                element_type,
                ndims,
            } => {
                if let Some(n) = ndims {
                    write!(f, "Array{{{}, {}}}", element_type, n)
                } else {
                    write!(f, "Array{{{}}}", element_type)
                }
            }
            JuliaType::Tuple(types) => {
                let inner: Vec<_> = types.iter().map(|t| format!("{}", t)).collect();
                write!(f, "Tuple{{{}}}", inner.join(", "))
            }
            JuliaType::Union(types) => {
                let inner: Vec<_> = types.iter().map(|t| format!("{}", t)).collect();
                write!(f, "Union{{{}}}", inner.join(", "))
            }
            JuliaType::Struct { name, type_params } => {
                if type_params.is_empty() {
                    write!(f, "{}", name)
                } else {
                    let params: Vec<_> = type_params.iter().map(|t| format!("{}", t)).collect();
                    write!(f, "{}{{{}}}", name, params.join(", "))
                }
            }
            JuliaType::TypeVar(name) => write!(f, "{}", name),
            JuliaType::Unknown => write!(f, "?"),
        }
    }
}

// ============================================================================
// StaticType - Static type representation for AoT code generation
// ============================================================================

/// Static type representation for AoT compilation
///
/// This enum represents types that have been statically inferred and can be
/// directly mapped to Rust types for code generation. Unlike `JuliaType`,
/// `StaticType` is designed specifically for tracking compile-time type
/// information with clear Rust equivalents.
///
/// # Type Levels
///
/// - **Fully Static**: All types are known at compile time (Level 0)
/// - **Inferred with Guards**: Types inferred but need runtime checks (Level 1)
/// - **Conditional**: Multiple possible types based on control flow (Level 2)
/// - **Dynamic**: Falls back to runtime dispatch (Level 3)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StaticType {
    // ========== Primitive Types ==========
    /// 64-bit signed integer (Julia Int64, Rust i64)
    I64,
    /// 32-bit signed integer (Julia Int32, Rust i32)
    I32,
    /// 16-bit signed integer (Julia Int16, Rust i16)
    I16,
    /// 8-bit signed integer (Julia Int8, Rust i8)
    I8,
    /// 64-bit unsigned integer (Julia UInt64, Rust u64)
    U64,
    /// 32-bit unsigned integer (Julia UInt32, Rust u32)
    U32,
    /// 16-bit unsigned integer (Julia UInt16, Rust u16)
    U16,
    /// 8-bit unsigned integer (Julia UInt8, Rust u8)
    U8,
    /// 64-bit floating point (Julia Float64, Rust f64)
    F64,
    /// 32-bit floating point (Julia Float32, Rust f32)
    F32,
    /// Boolean (Julia Bool, Rust bool)
    Bool,
    /// String (Julia String, Rust String)
    Str,
    /// Character (Julia Char, Rust char)
    Char,
    /// Nothing (Julia Nothing, Rust ())
    Nothing,
    /// Missing (Julia Missing, maps to Option::None at runtime)
    Missing,

    // ========== Container Types ==========
    /// Array with known element type
    Array {
        /// Element type
        element: Box<StaticType>,
        /// Number of dimensions (None = unknown)
        ndims: Option<usize>,
    },
    /// Tuple with known element types
    Tuple(Vec<StaticType>),
    /// Dictionary with known key/value types
    Dict {
        /// Key type
        key: Box<StaticType>,
        /// Value type
        value: Box<StaticType>,
    },
    /// Range type (Julia start:stop or start:step:stop)
    Range {
        /// Element type (typically I64)
        element: Box<StaticType>,
    },

    // ========== Struct Types ==========
    /// User-defined struct type
    Struct {
        /// Type ID (unique identifier)
        type_id: usize,
        /// Type name (e.g., "Point", "Complex{Float64}")
        name: String,
    },

    // ========== Function Types ==========
    /// Function type with known signature
    Function {
        /// Parameter types
        params: Vec<StaticType>,
        /// Return type
        ret: Box<StaticType>,
    },

    // ========== Union Types ==========
    /// Union of multiple possible types
    Union {
        /// Possible type variants
        variants: Vec<StaticType>,
    },

    // ========== Dynamic Type ==========
    /// Dynamic type (requires runtime dispatch)
    /// Used when type cannot be statically determined
    Any,
}

impl StaticType {
    /// Check if this type is fully static (no Any or Union types)
    ///
    /// Returns true if all type information is known at compile time.
    ///
    /// # Examples
    /// ```ignore
    /// use subset_julia_vm::aot::types::StaticType;
    ///
    /// assert!(StaticType::I64.is_fully_static());
    /// assert!(!StaticType::Any.is_fully_static());
    /// ```
    pub fn is_fully_static(&self) -> bool {
        match self {
            StaticType::Any => false,
            StaticType::Union { variants } => {
                // Union is fully static only if it has exactly one variant
                // and that variant is fully static
                variants.len() == 1 && variants[0].is_fully_static()
            }
            StaticType::Array { element, .. } => element.is_fully_static(),
            StaticType::Tuple(elements) => elements.iter().all(|e| e.is_fully_static()),
            StaticType::Dict { key, value } => key.is_fully_static() && value.is_fully_static(),
            StaticType::Range { element } => element.is_fully_static(),
            StaticType::Function { params, ret } => {
                params.iter().all(|p| p.is_fully_static()) && ret.is_fully_static()
            }
            StaticType::Struct { .. } => true,
            // All primitive types are fully static
            _ => true,
        }
    }

    /// Check if this is a primitive type
    ///
    /// Primitive types can be directly represented as Rust primitives.
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            StaticType::I64
                | StaticType::I32
                | StaticType::I16
                | StaticType::I8
                | StaticType::U64
                | StaticType::U32
                | StaticType::U16
                | StaticType::U8
                | StaticType::F64
                | StaticType::F32
                | StaticType::Bool
                | StaticType::Char
                | StaticType::Str
                | StaticType::Nothing
        )
    }

    /// Check if this is a numeric type
    ///
    /// In Julia, Bool is a subtype of Integer, which is a subtype of Number,
    /// so Bool is included as a numeric type for promotion purposes.
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            StaticType::I64
                | StaticType::I32
                | StaticType::I16
                | StaticType::I8
                | StaticType::U64
                | StaticType::U32
                | StaticType::U16
                | StaticType::U8
                | StaticType::F64
                | StaticType::F32
                | StaticType::Bool // Bool <: Integer <: Number
        )
    }

    /// Check if this is an integer type (including Bool)
    ///
    /// In Julia, Bool is a subtype of Integer:
    /// `julia> Bool <: Integer` returns `true`
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            StaticType::I64
                | StaticType::I32
                | StaticType::I16
                | StaticType::I8
                | StaticType::U64
                | StaticType::U32
                | StaticType::U16
                | StaticType::U8
                | StaticType::Bool // Bool <: Integer in Julia
        )
    }

    /// Check if this is a signed integer type
    pub fn is_signed(&self) -> bool {
        matches!(
            self,
            StaticType::I64 | StaticType::I32 | StaticType::I16 | StaticType::I8
        )
    }

    /// Check if this is an unsigned integer type
    pub fn is_unsigned(&self) -> bool {
        matches!(
            self,
            StaticType::U64 | StaticType::U32 | StaticType::U16 | StaticType::U8
        )
    }

    /// Check if this is a floating point type
    pub fn is_float(&self) -> bool {
        matches!(self, StaticType::F64 | StaticType::F32)
    }

    /// Check if this is an array type
    pub fn is_array(&self) -> bool {
        matches!(self, StaticType::Array { .. })
    }

    /// Check if this is a tuple type
    pub fn is_tuple(&self) -> bool {
        matches!(self, StaticType::Tuple(_))
    }

    /// Check if this is a range type
    pub fn is_range(&self) -> bool {
        matches!(self, StaticType::Range { .. })
    }

    /// Convert to Rust type name
    ///
    /// Returns the Rust type that corresponds to this Julia type.
    ///
    /// # Examples
    /// ```ignore
    /// use subset_julia_vm::aot::types::StaticType;
    ///
    /// assert_eq!(StaticType::I64.to_rust_type(), "i64");
    /// assert_eq!(StaticType::F64.to_rust_type(), "f64");
    /// ```
    pub fn to_rust_type(&self) -> String {
        match self {
            StaticType::I64 => "i64".to_string(),
            StaticType::I32 => "i32".to_string(),
            StaticType::I16 => "i16".to_string(),
            StaticType::I8 => "i8".to_string(),
            StaticType::U64 => "u64".to_string(),
            StaticType::U32 => "u32".to_string(),
            StaticType::U16 => "u16".to_string(),
            StaticType::U8 => "u8".to_string(),
            StaticType::F64 => "f64".to_string(),
            StaticType::F32 => "f32".to_string(),
            StaticType::Bool => "bool".to_string(),
            StaticType::Str => "String".to_string(),
            StaticType::Char => "char".to_string(),
            StaticType::Nothing => "()".to_string(),
            StaticType::Missing => "Option<Value>".to_string(),
            StaticType::Array { element, ndims } => {
                // For multidimensional arrays, generate nested Vec types
                // 1D: Vec<T>, 2D: Vec<Vec<T>>, etc.
                let dims = ndims.unwrap_or(1);
                let inner = element.to_rust_type();
                if dims <= 1 {
                    format!("Vec<{}>", inner)
                } else {
                    // Wrap in Vec<> for each dimension
                    let mut result = inner;
                    for _ in 0..dims {
                        result = format!("Vec<{}>", result);
                    }
                    result
                }
            }
            StaticType::Tuple(elements) => {
                let inner: Vec<_> = elements.iter().map(|e| e.to_rust_type()).collect();
                format!("({})", inner.join(", "))
            }
            StaticType::Dict { key, value } => {
                format!(
                    "std::collections::HashMap<{}, {}>",
                    key.to_rust_type(),
                    value.to_rust_type()
                )
            }
            StaticType::Range { element } => {
                format!("std::ops::Range<{}>", element.to_rust_type())
            }
            StaticType::Struct { name, .. } => {
                // Use the struct name as-is (assume it's been declared in generated code)
                name.clone()
            }
            StaticType::Function { params, ret } => {
                let param_types: Vec<_> = params.iter().map(|p| p.to_rust_type()).collect();
                format!("fn({}) -> {}", param_types.join(", "), ret.to_rust_type())
            }
            StaticType::Union { variants } => {
                if variants.len() == 1 {
                    variants[0].to_rust_type()
                } else {
                    // For unions, we fall back to Value
                    "Value".to_string()
                }
            }
            StaticType::Any => "Value".to_string(),
        }
    }

    /// Convert to Rust Result type name
    ///
    /// Returns the Rust type wrapped in RuntimeResult for error handling.
    ///
    /// # Examples
    /// ```ignore
    /// use subset_julia_vm::aot::types::StaticType;
    ///
    /// assert_eq!(StaticType::I64.to_rust_result_type(), "RuntimeResult<i64>");
    /// ```
    pub fn to_rust_result_type(&self) -> String {
        format!("RuntimeResult<{}>", self.to_rust_type())
    }

    /// Get the Julia type name
    pub fn julia_type_name(&self) -> String {
        match self {
            StaticType::I64 => "Int64".to_string(),
            StaticType::I32 => "Int32".to_string(),
            StaticType::I16 => "Int16".to_string(),
            StaticType::I8 => "Int8".to_string(),
            StaticType::U64 => "UInt64".to_string(),
            StaticType::U32 => "UInt32".to_string(),
            StaticType::U16 => "UInt16".to_string(),
            StaticType::U8 => "UInt8".to_string(),
            StaticType::F64 => "Float64".to_string(),
            StaticType::F32 => "Float32".to_string(),
            StaticType::Bool => "Bool".to_string(),
            StaticType::Str => "String".to_string(),
            StaticType::Char => "Char".to_string(),
            StaticType::Nothing => "Nothing".to_string(),
            StaticType::Missing => "Missing".to_string(),
            StaticType::Array { element, ndims } => {
                if let Some(n) = ndims {
                    format!("Array{{{}, {}}}", element.julia_type_name(), n)
                } else {
                    format!("Array{{{}}}", element.julia_type_name())
                }
            }
            StaticType::Tuple(elements) => {
                let inner: Vec<_> = elements.iter().map(|e| e.julia_type_name()).collect();
                format!("Tuple{{{}}}", inner.join(", "))
            }
            StaticType::Dict { key, value } => {
                format!(
                    "Dict{{{}, {}}}",
                    key.julia_type_name(),
                    value.julia_type_name()
                )
            }
            StaticType::Range { element } => {
                format!("UnitRange{{{}}}", element.julia_type_name())
            }
            StaticType::Struct { name, .. } => name.clone(),
            StaticType::Function { params, ret } => {
                let param_types: Vec<_> = params.iter().map(|p| p.julia_type_name()).collect();
                format!(
                    "Function{{({}) -> {}}}",
                    param_types.join(", "),
                    ret.julia_type_name()
                )
            }
            StaticType::Union { variants } => {
                let inner: Vec<_> = variants.iter().map(|v| v.julia_type_name()).collect();
                format!("Union{{{}}}", inner.join(", "))
            }
            StaticType::Any => "Any".to_string(),
        }
    }

    /// Get the mangled suffix for use in function names
    ///
    /// Returns a string suitable for appending to function names for type specialization.
    /// Used for multiple dispatch to create unique function names like `add_i64_i64`.
    ///
    /// # Examples
    /// ```ignore
    /// use subset_julia_vm::aot::types::StaticType;
    ///
    /// assert_eq!(StaticType::I64.mangle_suffix(), "i64");
    /// assert_eq!(StaticType::F64.mangle_suffix(), "f64");
    /// ```
    pub fn mangle_suffix(&self) -> String {
        match self {
            StaticType::I64 => "i64".to_string(),
            StaticType::I32 => "i32".to_string(),
            StaticType::I16 => "i16".to_string(),
            StaticType::I8 => "i8".to_string(),
            StaticType::U64 => "u64".to_string(),
            StaticType::U32 => "u32".to_string(),
            StaticType::U16 => "u16".to_string(),
            StaticType::U8 => "u8".to_string(),
            StaticType::F64 => "f64".to_string(),
            StaticType::F32 => "f32".to_string(),
            StaticType::Bool => "bool".to_string(),
            StaticType::Char => "char".to_string(),
            StaticType::Str => "str".to_string(),
            StaticType::Nothing => "nothing".to_string(),
            StaticType::Missing => "missing".to_string(),
            StaticType::Array { element, ndims } => {
                if let Some(n) = ndims {
                    format!("arr{}_{}", n, element.mangle_suffix())
                } else {
                    format!("arr_{}", element.mangle_suffix())
                }
            }
            StaticType::Tuple(elements) => {
                let inner: Vec<_> = elements.iter().map(|e| e.mangle_suffix()).collect();
                format!("tup_{}", inner.join("_"))
            }
            StaticType::Dict { key, value } => {
                format!("dict_{}_{}", key.mangle_suffix(), value.mangle_suffix())
            }
            StaticType::Range { element } => format!("range_{}", element.mangle_suffix()),
            StaticType::Struct { name, .. } => name.to_lowercase(),
            StaticType::Function { .. } => "fn".to_string(),
            StaticType::Union { .. } => "union".to_string(),
            StaticType::Any => "any".to_string(),
        }
    }
}

impl fmt::Display for StaticType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.julia_type_name())
    }
}

/// Convert from VM's JuliaType to StaticType
impl From<&crate::types::JuliaType> for StaticType {
    fn from(jt: &crate::types::JuliaType) -> Self {
        use crate::types::JuliaType as VmType;

        match jt {
            // Signed integers
            VmType::Int8 => StaticType::I8,
            VmType::Int16 => StaticType::I16,
            VmType::Int32 => StaticType::I32,
            VmType::Int64 => StaticType::I64,
            VmType::Int128 => StaticType::Any, // No direct Rust i128 support in runtime
            VmType::BigInt => StaticType::Any, // Arbitrary precision needs runtime

            // Unsigned integers
            VmType::UInt8 => StaticType::U8,
            VmType::UInt16 => StaticType::U16,
            VmType::UInt32 => StaticType::U32,
            VmType::UInt64 => StaticType::U64,
            VmType::UInt128 => StaticType::Any, // No direct Rust u128 support in runtime

            // Floating point
            VmType::Float16 => StaticType::F32, // Map to f32
            VmType::Float32 => StaticType::F32,
            VmType::Float64 => StaticType::F64,
            VmType::BigFloat => StaticType::Any, // Arbitrary precision needs runtime

            // Boolean
            VmType::Bool => StaticType::Bool,

            // String/Char
            VmType::String => StaticType::Str,
            VmType::Char => StaticType::Char,

            // Special types
            VmType::Nothing => StaticType::Nothing,
            VmType::Missing => StaticType::Missing,

            // Container types
            VmType::Array | VmType::VectorOf(_) | VmType::MatrixOf(_) => {
                // Get element type if available
                let element = match jt {
                    VmType::VectorOf(elem) => Box::new(StaticType::from(elem.as_ref())),
                    VmType::MatrixOf(elem) => Box::new(StaticType::from(elem.as_ref())),
                    _ => Box::new(StaticType::Any),
                };
                let ndims = match jt {
                    VmType::VectorOf(_) => Some(1),
                    VmType::MatrixOf(_) => Some(2),
                    _ => None,
                };
                StaticType::Array { element, ndims }
            }
            VmType::TupleOf(elements) => {
                StaticType::Tuple(elements.iter().map(StaticType::from).collect())
            }
            VmType::Tuple => StaticType::Tuple(vec![]),
            VmType::Dict => StaticType::Dict {
                key: Box::new(StaticType::Any),
                value: Box::new(StaticType::Any),
            },
            VmType::UnitRange | VmType::StepRange => StaticType::Range {
                element: Box::new(StaticType::I64),
            },

            // Struct types
            VmType::Struct(name) => StaticType::Struct {
                type_id: 0, // Type ID would be resolved during compilation
                name: name.clone(),
            },

            // Abstract types and others map to Any
            VmType::Any
            | VmType::Number
            | VmType::Real
            | VmType::Integer
            | VmType::Signed
            | VmType::Unsigned
            | VmType::AbstractFloat
            | VmType::AbstractString
            | VmType::AbstractChar
            | VmType::AbstractArray
            | VmType::AbstractRange
            | VmType::Function
            | VmType::IO
            | VmType::IOBuffer
            | VmType::Module
            | VmType::DataType
            | VmType::Type
            | VmType::Symbol
            | VmType::Expr
            | VmType::QuoteNode
            | VmType::LineNumberNode
            | VmType::GlobalRef
            | VmType::Pairs
            | VmType::Generator
            | VmType::Set
            | VmType::NamedTuple
            | VmType::AbstractUser(_, _)
            | VmType::TypeVar(_, _)
            | VmType::Bottom
            | VmType::TypeOf(_) => StaticType::Any,

            // Enum types are backed by Int32 in Julia
            VmType::Enum(_) => StaticType::I32,

            // Union types
            VmType::Union(types) => {
                let variants: Vec<_> = types.iter().map(StaticType::from).collect();
                if variants.iter().all(|v| matches!(v, StaticType::Any)) {
                    StaticType::Any
                } else {
                    StaticType::Union { variants }
                }
            }

            // UnionAll types (existentially quantified types like Vector{T} where T)
            VmType::UnionAll { body, .. } => {
                // For static compilation, we try to use the body type
                // The type parameter is existentially quantified
                StaticType::from(body.as_ref())
            }
        }
    }
}

/// Convert from VM's JuliaType to AoT's JuliaType
impl From<&crate::types::JuliaType> for JuliaType {
    fn from(jt: &crate::types::JuliaType) -> Self {
        use crate::types::JuliaType as VmType;

        match jt {
            // Signed integers
            VmType::Int8 => JuliaType::Int8,
            VmType::Int16 => JuliaType::Int16,
            VmType::Int32 => JuliaType::Int32,
            VmType::Int64 => JuliaType::Int64,
            VmType::Int128 => JuliaType::Int128,
            VmType::BigInt => JuliaType::Any,

            // Unsigned integers
            VmType::UInt8 => JuliaType::UInt8,
            VmType::UInt16 => JuliaType::UInt16,
            VmType::UInt32 => JuliaType::UInt32,
            VmType::UInt64 => JuliaType::UInt64,
            VmType::UInt128 => JuliaType::UInt128,

            // Floating point
            VmType::Float16 => JuliaType::Float16,
            VmType::Float32 => JuliaType::Float32,
            VmType::Float64 => JuliaType::Float64,
            VmType::BigFloat => JuliaType::Any,

            // Boolean
            VmType::Bool => JuliaType::Bool,

            // String/Char
            VmType::String => JuliaType::String,
            VmType::Char => JuliaType::Char,

            // Special types
            VmType::Nothing => JuliaType::Nothing,
            VmType::Missing => JuliaType::Missing,

            // Container types
            VmType::Array | VmType::VectorOf(_) | VmType::MatrixOf(_) => {
                let element = match jt {
                    VmType::VectorOf(elem) => Box::new(JuliaType::from(elem.as_ref())),
                    VmType::MatrixOf(elem) => Box::new(JuliaType::from(elem.as_ref())),
                    _ => Box::new(JuliaType::Any),
                };
                let ndims = match jt {
                    VmType::VectorOf(_) => Some(1),
                    VmType::MatrixOf(_) => Some(2),
                    _ => None,
                };
                JuliaType::Array {
                    element_type: element,
                    ndims,
                }
            }
            VmType::TupleOf(elements) => {
                JuliaType::Tuple(elements.iter().map(JuliaType::from).collect())
            }
            VmType::Tuple => JuliaType::Tuple(vec![]),

            // Struct types
            VmType::Struct(name) => JuliaType::Struct {
                name: name.clone(),
                type_params: vec![],
            },

            // Type variable
            VmType::TypeVar(name, _) => JuliaType::TypeVar(name.clone()),

            // Union types
            VmType::Union(types) => JuliaType::Union(types.iter().map(JuliaType::from).collect()),

            // UnionAll types
            VmType::UnionAll { body, .. } => JuliaType::from(body.as_ref()),

            // Everything else maps to Any
            _ => JuliaType::Any,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_julia_type_is_concrete() {
        assert!(JuliaType::Int64.is_concrete());
        assert!(JuliaType::Bool.is_concrete());
        assert!(!JuliaType::Any.is_concrete());
        assert!(!JuliaType::Unknown.is_concrete());
    }

    #[test]
    fn test_julia_type_is_numeric() {
        assert!(JuliaType::Int64.is_numeric());
        assert!(JuliaType::Float64.is_numeric());
        assert!(!JuliaType::Bool.is_numeric());
        assert!(!JuliaType::String.is_numeric());
    }

    #[test]
    fn test_julia_type_to_rust_type() {
        assert_eq!(JuliaType::Int64.to_rust_type(), "i64");
        assert_eq!(JuliaType::Float64.to_rust_type(), "f64");
        assert_eq!(JuliaType::Bool.to_rust_type(), "bool");
    }

    #[test]
    fn test_julia_type_display() {
        assert_eq!(format!("{}", JuliaType::Int64), "Int64");
        assert_eq!(
            format!(
                "{}",
                JuliaType::Array {
                    element_type: Box::new(JuliaType::Float64),
                    ndims: Some(2)
                }
            ),
            "Array{Float64, 2}"
        );
    }

    // ========== StaticType Tests ==========

    #[test]
    fn test_static_type_to_rust_type() {
        assert_eq!(StaticType::I64.to_rust_type(), "i64");
        assert_eq!(StaticType::F64.to_rust_type(), "f64");
        assert_eq!(StaticType::Bool.to_rust_type(), "bool");
        assert_eq!(StaticType::Str.to_rust_type(), "String");
        assert_eq!(StaticType::Nothing.to_rust_type(), "()");
        assert_eq!(StaticType::Any.to_rust_type(), "Value");
    }

    #[test]
    fn test_static_type_array_to_rust_type() {
        let arr = StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(1),
        };
        assert_eq!(arr.to_rust_type(), "Vec<i64>");
    }

    #[test]
    fn test_static_type_2d_array_to_rust_type() {
        // 2D array: Vec<Vec<i64>>
        let arr2d = StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(2),
        };
        assert_eq!(arr2d.to_rust_type(), "Vec<Vec<i64>>");

        // 3D array: Vec<Vec<Vec<f64>>>
        let arr3d = StaticType::Array {
            element: Box::new(StaticType::F64),
            ndims: Some(3),
        };
        assert_eq!(arr3d.to_rust_type(), "Vec<Vec<Vec<f64>>>");

        // Array with no ndims defaults to 1D
        let arr_default = StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: None,
        };
        assert_eq!(arr_default.to_rust_type(), "Vec<i64>");
    }

    #[test]
    fn test_static_type_to_rust_result_type() {
        assert_eq!(StaticType::I64.to_rust_result_type(), "RuntimeResult<i64>");
        assert_eq!(StaticType::F64.to_rust_result_type(), "RuntimeResult<f64>");
    }

    #[test]
    fn test_static_type_is_fully_static() {
        assert!(StaticType::I64.is_fully_static());
        assert!(StaticType::F64.is_fully_static());
        assert!(StaticType::Bool.is_fully_static());
        assert!(!StaticType::Any.is_fully_static());

        let arr = StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(1),
        };
        assert!(arr.is_fully_static());

        let arr_any = StaticType::Array {
            element: Box::new(StaticType::Any),
            ndims: None,
        };
        assert!(!arr_any.is_fully_static());
    }

    #[test]
    fn test_static_type_is_primitive() {
        assert!(StaticType::I64.is_primitive());
        assert!(StaticType::F64.is_primitive());
        assert!(StaticType::Bool.is_primitive());
        assert!(StaticType::Str.is_primitive());
        assert!(!StaticType::Any.is_primitive());

        let arr = StaticType::Array {
            element: Box::new(StaticType::I64),
            ndims: Some(1),
        };
        assert!(!arr.is_primitive());
    }

    #[test]
    fn test_static_type_is_numeric() {
        assert!(StaticType::I64.is_numeric());
        assert!(StaticType::I32.is_numeric());
        assert!(StaticType::F64.is_numeric());
        assert!(StaticType::F32.is_numeric());
        assert!(StaticType::U64.is_numeric());
        // In Julia: Bool <: Integer <: Number, so Bool is numeric
        assert!(StaticType::Bool.is_numeric());
        assert!(!StaticType::Str.is_numeric());
    }

    #[test]
    fn test_static_type_is_integer() {
        assert!(StaticType::I64.is_integer());
        assert!(StaticType::I32.is_integer());
        assert!(StaticType::U64.is_integer());
        assert!(!StaticType::F64.is_integer());
        // In Julia: Bool <: Integer, so Bool is an integer type
        assert!(StaticType::Bool.is_integer());
    }

    #[test]
    fn test_static_type_is_float() {
        assert!(StaticType::F64.is_float());
        assert!(StaticType::F32.is_float());
        assert!(!StaticType::I64.is_float());
        assert!(!StaticType::Bool.is_float());
    }

    #[test]
    fn test_static_type_display() {
        assert_eq!(format!("{}", StaticType::I64), "Int64");
        assert_eq!(format!("{}", StaticType::F64), "Float64");
        assert_eq!(format!("{}", StaticType::Any), "Any");

        let arr = StaticType::Array {
            element: Box::new(StaticType::F64),
            ndims: Some(2),
        };
        assert_eq!(format!("{}", arr), "Array{Float64, 2}");
    }

    #[test]
    fn test_static_type_from_vm_julia_type() {
        use crate::types::JuliaType as VmType;

        assert_eq!(StaticType::from(&VmType::Int64), StaticType::I64);
        assert_eq!(StaticType::from(&VmType::Float64), StaticType::F64);
        assert_eq!(StaticType::from(&VmType::Bool), StaticType::Bool);
        assert_eq!(StaticType::from(&VmType::String), StaticType::Str);
        assert_eq!(StaticType::from(&VmType::Any), StaticType::Any);
        // Enum types are backed by Int32
        assert_eq!(
            StaticType::from(&VmType::Enum("Color".to_string())),
            StaticType::I32
        );
    }

    #[test]
    fn test_static_type_struct() {
        let s = StaticType::Struct {
            type_id: 1,
            name: "Point{Float64}".to_string(),
        };
        assert!(s.is_fully_static());
        assert!(!s.is_primitive());
        assert_eq!(s.to_rust_type(), "Point{Float64}");
    }

    #[test]
    fn test_static_type_function() {
        let f = StaticType::Function {
            params: vec![StaticType::I64, StaticType::I64],
            ret: Box::new(StaticType::I64),
        };
        assert!(f.is_fully_static());
        assert_eq!(f.to_rust_type(), "fn(i64, i64) -> i64");
    }

    #[test]
    fn test_static_type_union() {
        let u = StaticType::Union {
            variants: vec![StaticType::I64, StaticType::F64],
        };
        assert!(!u.is_fully_static()); // Union with multiple variants is not fully static
        assert_eq!(u.to_rust_type(), "Value"); // Falls back to Value

        let single = StaticType::Union {
            variants: vec![StaticType::I64],
        };
        assert!(single.is_fully_static()); // Single-variant union is fully static
        assert_eq!(single.to_rust_type(), "i64");
    }
}
