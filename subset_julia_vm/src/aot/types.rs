//! Type definitions for AoT compilation
//!
//! This module defines [`StaticType`], the AoT carrier type used by the SSA IR
//! and code generation.
//!
//! `StaticType` is a lossy ABI/codegen projection from the shared `CoreType`
//! model, designed for code generation where we need exact Rust/ABI layouts. It
//! must not own Julia semantic rules such as subtyping, parametric matching, or
//! type joins; those live in `inference_core::CoreType` and are projected back
//! here only when a stable AoT layout exists.

use std::fmt;

// ============================================================================
// StaticType - Static type representation for AoT code generation
// ============================================================================

/// Static type representation for AoT compilation
///
/// This enum represents types that have been statically inferred and can be
/// directly mapped to Rust types for code generation. `StaticType` is designed
/// specifically for tracking compile-time type information with clear Rust
/// equivalents.
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
    /// 128-bit signed integer (Julia Int128, Rust i128)
    I128,
    /// 32-bit signed integer (Julia Int32, Rust i32)
    I32,
    /// 16-bit signed integer (Julia Int16, Rust i16)
    I16,
    /// 8-bit signed integer (Julia Int8, Rust i8)
    I8,
    /// 64-bit unsigned integer (Julia UInt64, Rust u64)
    U64,
    /// 128-bit unsigned integer (Julia UInt128, Rust u128)
    U128,
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
    /// 16-bit floating point (Julia Float16). Preserved for inference; codegen may widen.
    F16,
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
    /// Julia DataType / type object value.
    ///
    /// This is an explicit AoT carrier for first-class type values such as the
    /// result of `typeof(x)`. Rust backend codegen currently gates this value
    /// until a Julia-compatible DataType runtime representation exists.
    DataType,

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
    /// NamedTuple with known field names and element types, carried as a Rust tuple.
    NamedTuple(Vec<(String, StaticType)>),
    /// Dictionary with known key/value types
    Dict {
        /// Key type
        key: Box<StaticType>,
        /// Value type
        value: Box<StaticType>,
    },
    /// Set with known element type
    Set {
        /// Element type
        element: Box<StaticType>,
    },
    /// Range type (Julia start:stop or start:step:stop)
    Range {
        /// Element type (typically I64)
        element: Box<StaticType>,
    },
    /// Lazy generator expression with known yielded element type.
    Generator {
        /// Yielded element type
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
            StaticType::NamedTuple(fields) => fields.iter().all(|(_, ty)| ty.is_fully_static()),
            StaticType::Dict { key, value } => key.is_fully_static() && value.is_fully_static(),
            StaticType::Set { element }
            | StaticType::Range { element }
            | StaticType::Generator { element } => element.is_fully_static(),
            StaticType::Function { params, ret } => {
                params.iter().all(|p| p.is_fully_static()) && ret.is_fully_static()
            }
            StaticType::Struct { .. } | StaticType::DataType => true,
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
                | StaticType::I128
                | StaticType::I32
                | StaticType::I16
                | StaticType::I8
                | StaticType::U64
                | StaticType::U128
                | StaticType::U32
                | StaticType::U16
                | StaticType::U8
                | StaticType::F64
                | StaticType::F32
                | StaticType::F16
                | StaticType::Bool
                | StaticType::Char
                | StaticType::Str
                | StaticType::Nothing
        )
    }

    /// Bridge to the shared primitive-numeric taxonomy
    /// ([`crate::inference_core::PrimitiveNumeric`], Issue #3508).
    ///
    /// Returns `Some(_)` for primitive numeric variants, `None` for every
    /// non-primitive variant (`Array`, `Tuple`, `Struct`, `Union`, `Any`, …).
    /// This single conversion is the only place that maps between the AoT
    /// static-type representation and the canonical taxonomy used by both
    /// the VM-side and AoT-side classifiers, so the two pipelines can no
    /// longer drift on what counts as numeric / integer / float.
    pub fn primitive_numeric(&self) -> Option<crate::inference_core::PrimitiveNumeric> {
        crate::inference_core::CoreType::from(self).primitive_numeric()
    }

    /// Convert from the shared primitive-numeric taxonomy into AoT's
    /// `StaticType` without dropping widths that the inference layer can track.
    pub fn from_primitive_numeric(kind: crate::inference_core::PrimitiveNumeric) -> Self {
        use crate::inference_core::PrimitiveNumeric as P;
        match kind {
            P::Bool => StaticType::Bool,
            P::Int8 => StaticType::I8,
            P::Int16 => StaticType::I16,
            P::Int32 => StaticType::I32,
            P::Int64 => StaticType::I64,
            P::Int128 => StaticType::I128,
            P::UInt8 => StaticType::U8,
            P::UInt16 => StaticType::U16,
            P::UInt32 => StaticType::U32,
            P::UInt64 => StaticType::U64,
            P::UInt128 => StaticType::U128,
            P::Float16 => StaticType::F16,
            P::Float32 => StaticType::F32,
            P::Float64 => StaticType::F64,
        }
    }

    /// Project a shared semantic [`crate::inference_core::CoreType`] back into
    /// AoT's backend-oriented `StaticType` representation when the shape has
    /// a stable codegen projection.
    ///
    /// This is intentionally lossy: semantic supertypes widen to the dynamic
    /// `Any` carrier, while `UnionAll`, type variables, value parameters, and
    /// unknown user-defined type objects stay in `CoreType` and return `None`
    /// instead of forcing `StaticType` to own Julia type semantics.
    pub fn from_core_type_lossy(core: &crate::inference_core::CoreType) -> Option<Self> {
        use crate::inference_core::{
            type_core::CoreValueParam as V, CoreAbstract as A, CorePrimitive as P, CoreType as C,
        };

        Some(match core {
            C::Any => StaticType::Any,
            // `CoreType::Bottom` (Julia's `Union{}`) projects to the empty
            // union. This keeps a provably-disjoint intersection from silently
            // widening to a misleading concrete/`Any` backend type (Issue #3912).
            C::Bottom => StaticType::Union { variants: vec![] },
            C::Primitive(P::Bool) => StaticType::Bool,
            C::Primitive(P::Int8) => StaticType::I8,
            C::Primitive(P::Int16) => StaticType::I16,
            C::Primitive(P::Int32) => StaticType::I32,
            C::Primitive(P::Int64) => StaticType::I64,
            C::Primitive(P::Int128) => StaticType::I128,
            C::Primitive(P::UInt8) => StaticType::U8,
            C::Primitive(P::UInt16) => StaticType::U16,
            C::Primitive(P::UInt32) => StaticType::U32,
            C::Primitive(P::UInt64) => StaticType::U64,
            C::Primitive(P::UInt128) => StaticType::U128,
            C::Primitive(P::Float16) => StaticType::F16,
            C::Primitive(P::Float32) => StaticType::F32,
            C::Primitive(P::Float64) => StaticType::F64,
            C::Primitive(P::String) => StaticType::Str,
            C::Primitive(P::Char) => StaticType::Char,
            C::Primitive(P::Nothing) => StaticType::Nothing,
            C::Primitive(P::Missing) => StaticType::Missing,
            C::Abstract(A::DataType) | C::TypeOf(_) => StaticType::DataType,
            // StaticType is a backend-layout projection and deliberately has
            // no nominal abstract-type variants. Preserve enclosing structural
            // joins by widening abstract members to the dynamic carrier instead
            // of failing the entire projection (Issue #10865).
            C::Abstract(_) => StaticType::Any,
            C::Tuple(elements) => StaticType::Tuple(
                elements
                    .iter()
                    .map(Self::from_core_type_lossy)
                    .collect::<Option<Vec<_>>>()?,
            ),
            C::Union(types) => StaticType::Union {
                variants: types
                    .iter()
                    .map(Self::from_core_type_lossy)
                    .collect::<Option<Vec<_>>>()?,
            },
            C::Struct { name, params }
                if name == "Array" || name == "Vector" || name == "Matrix" =>
            {
                let element = match params.first() {
                    Some(param) => Self::from_core_type_lossy(param)?,
                    None => StaticType::Any,
                };
                let ndims = match name.as_str() {
                    "Vector" => Some(1),
                    "Matrix" => Some(2),
                    "Array" => match params.get(1) {
                        Some(C::Value(V::Int(n))) => usize::try_from(*n).ok(),
                        _ => None,
                    },
                    _ => None,
                };
                StaticType::Array {
                    element: Box::new(element),
                    ndims,
                }
            }
            C::Struct { name, params } if name == "Dict" && params.len() == 2 => StaticType::Dict {
                key: Box::new(Self::from_core_type_lossy(&params[0])?),
                value: Box::new(Self::from_core_type_lossy(&params[1])?),
            },
            C::Struct { name, params } if name == "Set" && params.len() == 1 => StaticType::Set {
                element: Box::new(Self::from_core_type_lossy(&params[0])?),
            },
            C::Struct { name, params }
                if (name == "UnitRange" || name == "AbstractRange") && params.len() == 1 =>
            {
                StaticType::Range {
                    element: Box::new(Self::from_core_type_lossy(&params[0])?),
                }
            }
            _ => return None,
        })
    }

    /// Project the VM's Julia type syntax through the shared `CoreType` bridge
    /// before falling back to AoT's backend representation (Issue #3912).
    pub fn from_vm_julia_type_lossy(jt: &crate::types::JuliaType) -> Option<Self> {
        Self::from_core_type_lossy(&crate::inference_core::CoreType::from(jt))
    }

    /// Parse a Julia type name through `CoreType` before projecting to AoT.
    pub fn from_julia_name_lossy(name: &str) -> Option<Self> {
        Self::from_core_type_lossy(&crate::inference_core::CoreType::from_julia_name(name))
    }

    /// Project a shared `TypeExpr` into AoT's backend-oriented `StaticType`.
    ///
    /// Concrete VM-side `JuliaType` values keep the existing `From<&JuliaType>`
    /// behavior so abstract types still widen to `Any`. Non-concrete
    /// expressions go through the shared `CoreType` name parser first, then
    /// fall back to a user-struct surface when no stable AoT projection exists.
    pub fn from_type_expr_lossy(type_expr: &crate::types::TypeExpr) -> Self {
        use crate::types::TypeExpr;

        match type_expr {
            TypeExpr::Concrete(jt) => StaticType::from(jt),
            TypeExpr::TypeVar(_) | TypeExpr::Parameterized { .. } | TypeExpr::RuntimeExpr(_) => {
                let rendered = type_expr.to_string();
                StaticType::from_julia_name_lossy(&rendered).unwrap_or(StaticType::Struct {
                    type_id: 0,
                    name: rendered,
                })
            }
        }
    }

    /// Join through the shared `CoreType` lattice and project the result back
    /// to AoT when possible. This keeps `StaticType` as a codegen projection
    /// while tuple / union normalization and semantic joins live in the shared
    /// inference core (Issue #3860).
    pub fn core_typejoin(&self, other: &Self) -> Option<Self> {
        let joined = crate::inference_core::CoreType::from(self)
            .typejoin(&crate::inference_core::CoreType::from(other));
        Self::from_core_type_lossy(&joined)
    }

    /// Intersect (meet) through the shared `CoreType` lattice and project the
    /// result back to AoT when possible. Mirrors [`Self::core_typejoin`] so
    /// AoT meet decisions reuse the same subtype/intersection semantics the
    /// VM/compiler paths use, rather than a local AoT recursion (Issue #3912).
    ///
    /// A provably-disjoint meet yields `CoreType::Bottom`, which projects to
    /// the empty union (`Union{}`). Returns `None` only when the narrowed
    /// `CoreType` has no stable AoT backend projection.
    pub fn core_typeintersect(&self, other: &Self) -> Option<Self> {
        let met = crate::inference_core::CoreType::from(self)
            .type_intersect(&crate::inference_core::CoreType::from(other));
        Self::from_core_type_lossy(&met)
    }

    /// Check if this is a numeric type.
    ///
    /// In Julia, Bool is a subtype of Integer, which is a subtype of Number,
    /// so Bool is included as a numeric type for promotion purposes.
    ///
    /// Issue #3508 — delegates to the canonical
    /// [`crate::inference_core::PrimitiveNumeric`] taxonomy via
    /// [`Self::primitive_numeric`]. Behaviour is unchanged.
    pub fn is_numeric(&self) -> bool {
        self.primitive_numeric().is_some_and(|p| p.is_numeric())
    }

    /// Check if this is an integer type (including Bool).
    ///
    /// In Julia, Bool is a subtype of Integer:
    /// `julia> Bool <: Integer` returns `true`.
    ///
    /// Delegates to [`crate::inference_core::PrimitiveNumeric::is_integer`]
    /// (Issue #3508).
    pub fn is_integer(&self) -> bool {
        self.primitive_numeric().is_some_and(|p| p.is_integer())
    }

    /// Check if this is a signed integer type.
    /// Delegates to the shared taxonomy (Issue #3508). Bool is **not**
    /// classified as signed (matches the prior behaviour).
    pub fn is_signed(&self) -> bool {
        self.primitive_numeric()
            .is_some_and(|p| p.is_signed_integer())
    }

    /// Check if this is an unsigned integer type.
    /// Delegates to the shared taxonomy (Issue #3508). Bool is **not**
    /// classified as unsigned (matches the prior behaviour).
    pub fn is_unsigned(&self) -> bool {
        self.primitive_numeric()
            .is_some_and(|p| p.is_unsigned_integer())
    }

    /// Check if this is a floating point type.
    /// Delegates to the shared taxonomy (Issue #3508).
    pub fn is_float(&self) -> bool {
        self.primitive_numeric().is_some_and(|p| p.is_float())
    }

    /// Integer bit width of a concrete integer StaticType (Bool excluded).
    fn integer_bits(&self) -> Option<u32> {
        Some(match self {
            StaticType::I8 | StaticType::U8 => 8,
            StaticType::I16 | StaticType::U16 => 16,
            StaticType::I32 | StaticType::U32 => 32,
            StaticType::I64 | StaticType::U64 => 64,
            StaticType::I128 | StaticType::U128 => 128,
            _ => return None,
        })
    }

    /// Julia numeric promotion over a MIXED-type argument list (Issue #10131):
    /// returns the promoted common type when the args are 2+ numeric
    /// (non-Bool) types that are not all equal — any `Float64` wins, then
    /// `Float32`; among integers the wider width wins and equal widths
    /// promote to unsigned (upstream `promote_type(Int64, UInt64) ==
    /// UInt64`). Returns `None` for same-type lists (callers keep the
    /// type-preserving path), Bool-containing lists (callers keep the
    /// dedicated Bool rule), or any non-numeric argument.
    pub fn promote_numeric_args(arg_types: &[StaticType]) -> Option<StaticType> {
        if arg_types.len() < 2 {
            return None;
        }
        if arg_types
            .iter()
            .any(|ty| !ty.is_numeric() || matches!(ty, StaticType::Bool))
        {
            return None;
        }
        if arg_types.iter().all(|ty| ty == &arg_types[0]) {
            return None;
        }
        if arg_types.iter().any(|ty| matches!(ty, StaticType::F64)) {
            return Some(StaticType::F64);
        }
        if arg_types.iter().any(|ty| ty.is_float()) {
            return Some(StaticType::F32);
        }
        let mut best = arg_types[0].clone();
        for ty in &arg_types[1..] {
            let (bw, tw) = (best.integer_bits()?, ty.integer_bits()?);
            if tw > bw || (tw == bw && ty.is_unsigned() && best.is_signed()) {
                best = ty.clone();
            }
        }
        Some(best)
    }

    /// Check if this is an array type
    pub fn is_array(&self) -> bool {
        matches!(self, StaticType::Array { .. })
    }

    /// Check if this is a tuple type
    pub fn is_tuple(&self) -> bool {
        matches!(self, StaticType::Tuple(_) | StaticType::NamedTuple(_))
    }

    /// Check if this is a range type
    pub fn is_range(&self) -> bool {
        matches!(self, StaticType::Range { .. })
    }

    /// Check if this is a set type
    pub fn is_set(&self) -> bool {
        matches!(self, StaticType::Set { .. })
    }

    /// Check if this is a dict type
    pub fn is_dict(&self) -> bool {
        matches!(self, StaticType::Dict { .. })
    }

    /// Check if this is a lazy generator type
    pub fn is_generator(&self) -> bool {
        matches!(self, StaticType::Generator { .. })
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
            StaticType::I128 => "i128".to_string(),
            StaticType::I32 => "i32".to_string(),
            StaticType::I16 => "i16".to_string(),
            StaticType::I8 => "i8".to_string(),
            StaticType::U64 => "u64".to_string(),
            StaticType::U128 => "u128".to_string(),
            StaticType::U32 => "u32".to_string(),
            StaticType::U16 => "u16".to_string(),
            StaticType::U8 => "u8".to_string(),
            StaticType::F64 => "f64".to_string(),
            StaticType::F32 => "f32".to_string(),
            StaticType::F16 => "f32".to_string(),
            StaticType::Bool => "bool".to_string(),
            StaticType::Str => "String".to_string(),
            StaticType::Char => "char".to_string(),
            StaticType::Nothing => "()".to_string(),
            StaticType::Missing => "Value".to_string(),
            StaticType::DataType => "Value".to_string(),
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
                if inner.len() == 1 {
                    format!("({},)", inner[0])
                } else {
                    format!("({})", inner.join(", "))
                }
            }
            StaticType::NamedTuple(fields) => {
                let inner: Vec<_> = fields.iter().map(|(_, ty)| ty.to_rust_type()).collect();
                if inner.len() == 1 {
                    format!("({},)", inner[0])
                } else {
                    format!("({})", inner.join(", "))
                }
            }
            StaticType::Dict { key, value } => {
                format!(
                    "std::collections::HashMap<{}, {}>",
                    key.to_rust_type(),
                    value.to_rust_type()
                )
            }
            StaticType::Set { element } => {
                format!("std::collections::HashSet<{}>", element.to_rust_type())
            }
            StaticType::Range { element } if matches!(element.as_ref(), StaticType::Char) => {
                "SjuliaCharRange".to_string()
            }
            StaticType::Range { element } => {
                format!("SjuliaRange<{}>", element.to_rust_type())
            }
            StaticType::Generator { element } => {
                format!("Box<dyn Iterator<Item = {}>>", element.to_rust_type())
            }
            StaticType::Struct { name, .. } if name == "Complex" => "Complex".to_string(),
            StaticType::Struct { name, .. } => {
                // Issue #10907: bind each helper's `Option` once instead of
                // re-deriving it with a match-guard `.is_some()` check
                // followed by a body-side panicking assertion on a second
                // call to the same helper — same result, no panic path at all.
                if let (Some(inner), true) = (
                    Self::complex_param_rust_type_name(name),
                    Self::parametric_type_parts(name).is_none(),
                ) {
                    format!("Complex<{}>", inner)
                } else if let Some(rust_name) = Self::parametric_rust_type_name(name) {
                    rust_name
                } else {
                    // Use the struct name as-is (assume it's been declared in generated code)
                    name.clone()
                }
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

    pub(crate) fn complex_param_type_from_name(name: &str) -> Option<StaticType> {
        let param = Self::complex_param_name(name)?;
        match param {
            "Float64" => Some(StaticType::F64),
            "Float32" => Some(StaticType::F32),
            "Int64" => Some(StaticType::I64),
            "Int32" => Some(StaticType::I32),
            "Int16" => Some(StaticType::I16),
            "Int8" => Some(StaticType::I8),
            "UInt64" => Some(StaticType::U64),
            "UInt32" => Some(StaticType::U32),
            "UInt16" => Some(StaticType::U16),
            "UInt8" => Some(StaticType::U8),
            _ => None,
        }
    }

    pub(crate) fn complex_param_rust_type_name(name: &str) -> Option<&'static str> {
        match Self::complex_param_type_from_name(name)? {
            StaticType::F64 => Some("f64"),
            StaticType::F32 => Some("f32"),
            StaticType::I64 => Some("i64"),
            StaticType::I32 => Some("i32"),
            StaticType::I16 => Some("i16"),
            StaticType::I8 => Some("i8"),
            StaticType::U64 => Some("u64"),
            StaticType::U32 => Some("u32"),
            StaticType::U16 => Some("u16"),
            StaticType::U8 => Some("u8"),
            _ => None,
        }
    }

    pub(crate) fn parametric_type_parts(name: &str) -> Option<(&str, Vec<&str>)> {
        let open = name.find('{')?;
        let base = name[..open].trim();
        let rest = name[open + 1..].strip_suffix('}')?;
        if base.is_empty() {
            return None;
        }

        let mut params = Vec::new();
        let mut depth = 0usize;
        let mut start = 0usize;
        for (idx, ch) in rest.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => depth = depth.checked_sub(1)?,
                ',' if depth == 0 => {
                    let param = rest[start..idx].trim();
                    if param.is_empty() {
                        return None;
                    }
                    params.push(param);
                    start = idx + ch.len_utf8();
                }
                _ => {}
            }
        }
        if depth != 0 {
            return None;
        }
        let param = rest[start..].trim();
        if param.is_empty() {
            return None;
        }
        params.push(param);

        Some((base, params))
    }

    pub(crate) fn parametric_arg_static_type(name: &str) -> Option<StaticType> {
        StaticType::from_julia_name_lossy(name).or_else(|| {
            if Self::parametric_type_parts(name).is_some() {
                Some(StaticType::Struct {
                    type_id: 0,
                    name: name.to_string(),
                })
            } else {
                None
            }
        })
    }

    pub(crate) fn parametric_arg_rust_type_name(name: &str) -> Option<String> {
        Self::parametric_arg_static_type(name)
            .map(|ty| ty.to_rust_type())
            .or_else(|| {
                if name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                {
                    Some(name.to_string())
                } else {
                    None
                }
            })
    }

    pub(crate) fn parametric_rust_type_name(name: &str) -> Option<String> {
        let (base, params) = Self::parametric_type_parts(name)?;
        let rust_params: Vec<_> = params
            .iter()
            .map(|param| Self::parametric_arg_rust_type_name(param))
            .collect::<Option<_>>()?;
        Some(format!("{}<{}>", base, rust_params.join(", ")))
    }

    pub(crate) fn parametric_rust_constructor_path(name: &str) -> Option<String> {
        let (base, params) = Self::parametric_type_parts(name)?;
        let rust_params: Vec<_> = params
            .iter()
            .map(|param| Self::parametric_arg_rust_type_name(param))
            .collect::<Option<_>>()?;
        Some(format!("{}::<{}>", base, rust_params.join(", ")))
    }

    fn complex_param_name(name: &str) -> Option<&str> {
        match name {
            "ComplexF64" | "Complex{Float64}" => Some("Float64"),
            "ComplexF32" | "Complex{Float32}" => Some("Float32"),
            _ => name
                .strip_prefix("Complex{")
                .and_then(|rest| rest.strip_suffix('}')),
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
            StaticType::I128 => "Int128".to_string(),
            StaticType::I32 => "Int32".to_string(),
            StaticType::I16 => "Int16".to_string(),
            StaticType::I8 => "Int8".to_string(),
            StaticType::U64 => "UInt64".to_string(),
            StaticType::U128 => "UInt128".to_string(),
            StaticType::U32 => "UInt32".to_string(),
            StaticType::U16 => "UInt16".to_string(),
            StaticType::U8 => "UInt8".to_string(),
            StaticType::F64 => "Float64".to_string(),
            StaticType::F32 => "Float32".to_string(),
            StaticType::F16 => "Float16".to_string(),
            StaticType::Bool => "Bool".to_string(),
            StaticType::Str => "String".to_string(),
            StaticType::Char => "Char".to_string(),
            StaticType::Nothing => "Nothing".to_string(),
            StaticType::Missing => "Missing".to_string(),
            StaticType::DataType => "DataType".to_string(),
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
            StaticType::NamedTuple(fields) => {
                let inner: Vec<_> = fields
                    .iter()
                    .map(|(name, ty)| format!("{} = {}", name, ty.julia_type_name()))
                    .collect();
                format!("@NamedTuple{{{}}}", inner.join(", "))
            }
            StaticType::Dict { key, value } => {
                format!(
                    "Dict{{{}, {}}}",
                    key.julia_type_name(),
                    value.julia_type_name()
                )
            }
            StaticType::Set { element } => {
                format!("Set{{{}}}", element.julia_type_name())
            }
            StaticType::Range { element } => {
                format!("UnitRange{{{}}}", element.julia_type_name())
            }
            StaticType::Generator { element } => {
                format!("Base.Generator{{{}}}", element.julia_type_name())
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
            StaticType::I128 => "i128".to_string(),
            StaticType::I32 => "i32".to_string(),
            StaticType::I16 => "i16".to_string(),
            StaticType::I8 => "i8".to_string(),
            StaticType::U64 => "u64".to_string(),
            StaticType::U128 => "u128".to_string(),
            StaticType::U32 => "u32".to_string(),
            StaticType::U16 => "u16".to_string(),
            StaticType::U8 => "u8".to_string(),
            StaticType::F64 => "f64".to_string(),
            StaticType::F32 => "f32".to_string(),
            StaticType::F16 => "f16".to_string(),
            StaticType::Bool => "bool".to_string(),
            StaticType::Char => "char".to_string(),
            StaticType::Str => "str".to_string(),
            StaticType::Nothing => "nothing".to_string(),
            StaticType::Missing => "missing".to_string(),
            StaticType::DataType => "datatype".to_string(),
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
            StaticType::NamedTuple(fields) => {
                let inner: Vec<_> = fields
                    .iter()
                    .map(|(name, ty)| format!("{}_{}", name, ty.mangle_suffix()))
                    .collect();
                format!("nt_{}", inner.join("_"))
            }
            StaticType::Dict { key, value } => {
                format!("dict_{}_{}", key.mangle_suffix(), value.mangle_suffix())
            }
            StaticType::Set { element } => format!("set_{}", element.mangle_suffix()),
            StaticType::Range { element } => format!("range_{}", element.mangle_suffix()),
            StaticType::Generator { element } => format!("generator_{}", element.mangle_suffix()),
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

        if let Some(projected) = StaticType::from_vm_julia_type_lossy(jt) {
            return projected;
        }

        // Anything with a stable `CoreType` projection (every primitive, the
        // `Vector{T}`/`Matrix{T}`/`Array` family, parameterized tuples/unions,
        // and projectable ranges/dicts) has already returned above via
        // `from_vm_julia_type_lossy`. The arms below are only the residual
        // fallbacks for shapes the shared projection deliberately leaves in
        // `CoreType` (`None`): bigints, bare `Tuple`/`Dict`/range shells,
        // unknown user structs, enums, abstract families, and `UnionAll`
        // (Issue #6598).
        match jt {
            VmType::BigInt => StaticType::Any, // Arbitrary precision needs runtime
            VmType::BigFloat => StaticType::Any, // Arbitrary precision needs runtime
            VmType::Tuple => StaticType::Tuple(vec![]),
            VmType::Dict => StaticType::Dict {
                key: Box::new(StaticType::Any),
                value: Box::new(StaticType::Any),
            },
            VmType::Set => StaticType::Set {
                element: Box::new(StaticType::Any),
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
            VmType::DataType | VmType::TypeOf(_) => StaticType::DataType,

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
            | VmType::Type
            | VmType::Symbol
            | VmType::Expr
            | VmType::QuoteNode
            | VmType::LineNumberNode
            | VmType::GlobalRef
            | VmType::Pairs
            | VmType::Generator
            | VmType::NamedTuple
            | VmType::AbstractUser(_, _)
            | VmType::TypeVar(_, _)
            | VmType::Bottom => StaticType::Any,

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
            _ => StaticType::Any,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== StaticType Tests ==========

    #[test]
    fn test_static_type_to_rust_type() {
        assert_eq!(StaticType::I64.to_rust_type(), "i64");
        assert_eq!(StaticType::F64.to_rust_type(), "f64");
        assert_eq!(StaticType::Bool.to_rust_type(), "bool");
        assert_eq!(StaticType::Str.to_rust_type(), "String");
        assert_eq!(StaticType::Nothing.to_rust_type(), "()");
        assert_eq!(StaticType::DataType.to_rust_type(), "Value");
        assert_eq!(StaticType::Any.to_rust_type(), "Value");
    }

    #[test]
    fn datatype_static_type_is_explicit_issue_6973() {
        assert_eq!(StaticType::DataType.julia_type_name(), "DataType");
        assert_eq!(StaticType::DataType.mangle_suffix(), "datatype");
        assert_eq!(
            StaticType::from_core_type_lossy(&crate::inference_core::CoreType::TypeOf(Box::new(
                crate::inference_core::CoreType::Primitive(
                    crate::inference_core::CorePrimitive::Int64
                )
            ))),
            Some(StaticType::DataType)
        );
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
    fn test_static_type_set_to_rust_type_issue_7035() {
        let set = StaticType::Set {
            element: Box::new(StaticType::I64),
        };
        assert_eq!(set.to_rust_type(), "std::collections::HashSet<i64>");
        assert_eq!(set.julia_type_name(), "Set{Int64}");
        assert_eq!(set.mangle_suffix(), "set_i64");
    }

    #[test]
    fn test_static_type_dict_to_rust_type_issue_7034() {
        let dict = StaticType::Dict {
            key: Box::new(StaticType::Str),
            value: Box::new(StaticType::I64),
        };
        assert_eq!(
            dict.to_rust_type(),
            "std::collections::HashMap<String, i64>"
        );
        assert_eq!(dict.julia_type_name(), "Dict{String, Int64}");
        assert_eq!(dict.mangle_suffix(), "dict_str_i64");
        assert!(dict.is_dict());
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
    fn test_issue_6598_array_projections_route_through_core_type() {
        // #6598: the bare `Array` and `MatrixOf` arms of conversion #7
        // (`From<&vm::JuliaType> for StaticType`) duplicated what the shared
        // `CoreType` projection (`from_vm_julia_type_lossy`) already produces.
        // Pin that the modern CoreType-routed path yields the exact backend
        // shapes, so the redundant manual arms can be dropped without changing
        // any AoT projection.
        use crate::types::JuliaType as VmType;

        // Bare `Array` (no element / ndims) projects to `Array{Any}` with an
        // unknown rank entirely through CoreType.
        assert_eq!(
            StaticType::from_vm_julia_type_lossy(&VmType::Array),
            Some(StaticType::Array {
                element: Box::new(StaticType::Any),
                ndims: None,
            })
        );
        // `Matrix{T}` carries its element and the rank-2 ndims through CoreType.
        assert_eq!(
            StaticType::from_vm_julia_type_lossy(&VmType::MatrixOf(Box::new(VmType::Float64))),
            Some(StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(2),
            })
        );
        // `Vector{T}` was never duplicated in the manual fallback — it has only
        // ever resolved through CoreType; pin it alongside for completeness.
        assert_eq!(
            StaticType::from_vm_julia_type_lossy(&VmType::VectorOf(Box::new(VmType::Int64))),
            Some(StaticType::Array {
                element: Box::new(StaticType::I64),
                ndims: Some(1),
            })
        );

        // The public `From` entry point yields the same shapes (the modern path
        // wins before the manual fallback is ever consulted).
        assert_eq!(
            StaticType::from(&VmType::Array),
            StaticType::Array {
                element: Box::new(StaticType::Any),
                ndims: None,
            }
        );
        assert_eq!(
            StaticType::from(&VmType::MatrixOf(Box::new(VmType::Float64))),
            StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(2),
            }
        );
    }

    #[test]
    fn test_issue_3912_static_type_projection_uses_core_type() {
        use crate::inference_core::CoreType;
        use crate::types::JuliaType as VmType;
        use crate::types::TypeExpr;

        assert_eq!(
            StaticType::from_vm_julia_type_lossy(&VmType::Int8),
            Some(StaticType::I8)
        );
        assert_eq!(
            StaticType::from_vm_julia_type_lossy(&VmType::Int16),
            Some(StaticType::I16)
        );
        assert_eq!(
            StaticType::from_vm_julia_type_lossy(&VmType::UInt128),
            Some(StaticType::U128)
        );
        assert_eq!(
            StaticType::from_vm_julia_type_lossy(&VmType::Float16),
            Some(StaticType::F16)
        );

        assert_eq!(
            StaticType::from_vm_julia_type_lossy(&VmType::VectorOf(Box::new(VmType::Int64))),
            Some(StaticType::Array {
                element: Box::new(StaticType::I64),
                ndims: Some(1),
            })
        );
        assert_eq!(
            StaticType::from_vm_julia_type_lossy(&VmType::MatrixOf(Box::new(VmType::Float64))),
            Some(StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(2),
            })
        );
        assert_eq!(
            StaticType::from_julia_name_lossy("Array{Int64, 2}"),
            Some(StaticType::Array {
                element: Box::new(StaticType::I64),
                ndims: Some(2),
            })
        );
        assert_eq!(
            StaticType::from_julia_name_lossy("Matrix"),
            Some(StaticType::Array {
                element: Box::new(StaticType::Any),
                ndims: Some(2),
            })
        );
        assert_eq!(
            StaticType::from_julia_name_lossy("Tuple{Int64, String}"),
            Some(StaticType::Tuple(vec![StaticType::I64, StaticType::Str]))
        );
        assert_eq!(
            StaticType::from_julia_name_lossy("Union{Int64, Float64}"),
            Some(StaticType::Union {
                variants: vec![StaticType::I64, StaticType::F64],
            })
        );

        assert_eq!(
            StaticType::from_julia_name_lossy("Type{Int64}"),
            Some(StaticType::DataType)
        );
        assert_eq!(
            StaticType::from_julia_name_lossy("Real"),
            Some(StaticType::Any)
        );
        assert_eq!(
            StaticType::from_julia_name_lossy("Tuple{Real, Any}"),
            Some(StaticType::Tuple(vec![StaticType::Any, StaticType::Any]))
        );
        assert_eq!(
            StaticType::from_type_expr_lossy(&TypeExpr::Parameterized {
                base: "Array".to_string(),
                params: vec![
                    TypeExpr::Concrete(VmType::Int64),
                    TypeExpr::TypeVar("2".to_string()),
                ],
            }),
            StaticType::Array {
                element: Box::new(StaticType::I64),
                ndims: Some(2),
            }
        );
        assert_eq!(
            StaticType::from_type_expr_lossy(&TypeExpr::Parameterized {
                base: "Tuple".to_string(),
                params: vec![
                    TypeExpr::Concrete(VmType::Int64),
                    TypeExpr::Concrete(VmType::String),
                ],
            }),
            StaticType::Tuple(vec![StaticType::I64, StaticType::Str])
        );
        assert_eq!(
            StaticType::from_type_expr_lossy(&TypeExpr::Concrete(VmType::Real)),
            StaticType::Any
        );
        assert_eq!(
            StaticType::from_type_expr_lossy(&TypeExpr::TypeVar("T".to_string())),
            StaticType::Struct {
                type_id: 0,
                name: "T".to_string(),
            }
        );
        assert_eq!(
            StaticType::from_type_expr_lossy(&TypeExpr::RuntimeExpr("Symbol(s)".to_string())),
            StaticType::Struct {
                type_id: 0,
                name: "Symbol(s)".to_string(),
            }
        );
        assert_eq!(
            StaticType::from_core_type_lossy(&CoreType::from(&VmType::UnionAll {
                var: "T".to_string(),
                lower_bound: None,
                bound: Some(Box::new("Integer".to_string())),
                body: Box::new(VmType::VectorOf(Box::new(VmType::TypeVar(
                    "T".to_string(),
                    Some("Integer".to_string()),
                )))),
            })),
            None
        );
    }

    #[test]
    fn test_issue_3912_core_typeintersect_projects_through_core_type() {
        // Narrowing: Union{Int64, Float64} ∩ Int64 == Int64.
        assert_eq!(
            StaticType::Union {
                variants: vec![StaticType::I64, StaticType::F64],
            }
            .core_typeintersect(&StaticType::I64),
            Some(StaticType::I64)
        );

        // Provable disjointness projects CoreType::Bottom to the empty union
        // (Julia's `Union{}`), never to a misleading concrete/`Any` backend type.
        assert_eq!(
            StaticType::I64.core_typeintersect(&StaticType::F64),
            Some(StaticType::Union { variants: vec![] })
        );

        // Tuple intersection narrows element-wise.
        assert_eq!(
            StaticType::Tuple(vec![StaticType::I64, StaticType::F64])
                .core_typeintersect(&StaticType::Tuple(vec![StaticType::I64, StaticType::F64])),
            Some(StaticType::Tuple(vec![StaticType::I64, StaticType::F64]))
        );

        // Shapes without a stable AoT projection (a struct-vs-primitive meet
        // that lands on a non-projectable CoreType) report `None` rather than
        // forcing StaticType to own the semantics.
        assert_eq!(
            StaticType::Struct {
                type_id: 0,
                name: "BigInt".to_string(),
            }
            .core_typeintersect(&StaticType::Struct {
                type_id: 0,
                name: "BigFloat".to_string(),
            }),
            Some(StaticType::Union { variants: vec![] })
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
        assert_eq!(s.to_rust_type(), "Point<f64>");
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

// `StaticType -> CoreType` bridge (ADR_BACKEND_STRATEGY.md consequence 1,
// CRATE_SPLIT.md §4.3): lives on the AoT side because `CoreType` moved to
// `subset_julia_vm_types` (Issue #8655) and `_types` must stay free of AoT
// dependencies. `impl From<&LocalType> for ForeignType` is orphan-rule-legal.
// Relocated verbatim from `inference_core/type_core/convert.rs`.
impl From<&StaticType> for crate::inference_core::CoreType {
    fn from(ty: &StaticType) -> Self {
        use crate::inference_core::{CoreAbstract, CorePrimitive};
        use StaticType as ST;
        match ty {
            ST::I64 => Self::Primitive(CorePrimitive::Int64),
            ST::I128 => Self::Primitive(CorePrimitive::Int128),
            ST::I32 => Self::Primitive(CorePrimitive::Int32),
            ST::I16 => Self::Primitive(CorePrimitive::Int16),
            ST::I8 => Self::Primitive(CorePrimitive::Int8),
            ST::U64 => Self::Primitive(CorePrimitive::UInt64),
            ST::U128 => Self::Primitive(CorePrimitive::UInt128),
            ST::U32 => Self::Primitive(CorePrimitive::UInt32),
            ST::U16 => Self::Primitive(CorePrimitive::UInt16),
            ST::U8 => Self::Primitive(CorePrimitive::UInt8),
            ST::F64 => Self::Primitive(CorePrimitive::Float64),
            ST::F32 => Self::Primitive(CorePrimitive::Float32),
            ST::F16 => Self::Primitive(CorePrimitive::Float16),
            ST::Bool => Self::Primitive(CorePrimitive::Bool),
            ST::Str => Self::Primitive(CorePrimitive::String),
            ST::Char => Self::Primitive(CorePrimitive::Char),
            ST::Nothing => Self::Primitive(CorePrimitive::Nothing),
            ST::Missing => Self::Primitive(CorePrimitive::Missing),
            ST::DataType => Self::Abstract(CoreAbstract::DataType),
            ST::Any => Self::Any,
            ST::Array { element, .. } => Self::Struct {
                name: "Array".to_string(),
                params: vec![Self::from(element.as_ref())],
            },
            ST::Dict { key, value } => Self::Struct {
                name: "Dict".to_string(),
                params: vec![Self::from(key.as_ref()), Self::from(value.as_ref())],
            },
            ST::Set { element } => Self::Struct {
                name: "Set".to_string(),
                params: vec![Self::from(element.as_ref())],
            },
            ST::Tuple(elements) => Self::Tuple(elements.iter().map(Self::from).collect()),
            ST::NamedTuple(fields) => Self::NamedTuple(
                fields
                    .iter()
                    .map(|(name, ty)| (name.clone(), Self::from(ty)))
                    .collect(),
            ),
            ST::Union { variants } => Self::Union(variants.iter().map(Self::from).collect()),
            ST::Struct { name, .. } => Self::from_julia_name(name),
            ST::Function { .. } => Self::Abstract(CoreAbstract::Function),
            ST::Range { element } => Self::Struct {
                name: "AbstractRange".to_string(),
                params: vec![Self::from(element.as_ref())],
            },
            ST::Generator { element } => Self::Struct {
                name: "Base.Generator".to_string(),
                params: vec![Self::from(element.as_ref())],
            },
        }
    }
}
