//! Dictionary value type, keys, and hashing.
//!
//! Split out of `container.rs` by value kind (Issue #6835).

// SAFETY: the isize→usize casts in `DictValue::insert` are guarded by
// `if pos >= 0` and `(-avail - 1)` patterns that ensure non-negative values;
// the i128/f64→u128 casts in numeric key hashing operate on values already
// brought into the non-negative domain.
#![allow(clippy::cast_sign_loss)]

use super::super::error::VmError;
use super::Value;
use half::f16;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Dictionary key: supports strings, symbols, and numeric scalar keys.
#[derive(Debug, Clone, Eq)]
pub enum DictKey {
    Str(String),
    F16(u16),
    F32(u32),
    F64(u64),
    I64(i64),
    I32(i32),
    I16(i16),
    I8(i8),
    I128(i128),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Symbol(String),
    /// Type objects as Dict/Set keys (Issue #5108). `canonical` is a
    /// structural, registry-free hash of the type so equal types (even when
    /// reached through different `JuliaType` projections, e.g. `VectorOf(Int)`
    /// vs `Struct("Vector{Int64}")`) collapse to the same key, while distinct
    /// types (almost surely) do not. `original` round-trips back to the exact
    /// type-object `Value` for `to_value` / iteration.
    Type(TypeDictKey),
    /// Composite (tuple / named-tuple / svec / struct) keys (Issue #6693).
    /// Like `Type`, `canonical` is a structural hash used for hashing/bucketing
    /// and `original` round-trips back to the exact `Value`. Equality compares
    /// the structural `Debug` of `original` (not just the hash), so a hash
    /// collision never reports two distinct keys as equal. The `original` must be
    /// heap-free (no `Value::StructRef`): callers resolve heap struct refs to
    /// inline snapshots before `from_value`, so equal struct-bearing keys built
    /// separately (e.g. `(OneTo(3),)`) collapse to the same key.
    Composite(CompositeDictKey),
}

/// Composite Dict/Set key payload for tuple / named-tuple / svec / struct keys
/// (Issue #6693). Mirrors [`TypeDictKey`]: `canonical` (a structural hash of the
/// resolved value) drives hashing while `original` is retained for equality and
/// projection back to the key `Value`.
#[derive(Debug, Clone)]
pub struct CompositeDictKey {
    canonical: u64,
    original: Box<Value>,
}

impl PartialEq for CompositeDictKey {
    fn eq(&self, other: &Self) -> bool {
        // Hash first (cheap reject), then a full structural `Debug` compare so a
        // 64-bit collision cannot alias two distinct keys (unlike `TypeDictKey`,
        // where the canonical hash is the identity). `original` is heap-free, so
        // its `Debug` is a deterministic structural rendering.
        self.canonical == other.canonical
            && format!("{:?}", self.original) == format!("{:?}", other.original)
    }
}

impl Eq for CompositeDictKey {}

impl CompositeDictKey {
    /// Build a composite key from a tuple / named-tuple / svec / struct `Value`.
    /// Returns `None` for any other value, or if a `Value::StructRef` is
    /// reachable (callers must resolve heap struct refs first, so a stray ref is
    /// a bug — fail loudly to `InvalidDictKey` rather than hash a heap index).
    fn from_value(value: &Value) -> Option<Self> {
        if !matches!(
            value,
            Value::Tuple(_) | Value::SimpleVector(_) | Value::NamedTuple(_) | Value::Struct(_)
        ) {
            return None;
        }
        if composite_value_has_structref(value) {
            return None;
        }
        let mut hasher = DefaultHasher::new();
        // The structural `Debug` of a heap-free value is deterministic and
        // distinguishes values exactly; hashing it keeps eq/hash consistent.
        format!("{:?}", value).hash(&mut hasher);
        Some(Self {
            canonical: hasher.finish(),
            original: Box::new(value.clone()),
        })
    }
}

/// `true` if `value` reaches a heap `Value::StructRef` through tuple / svec /
/// named-tuple elements or inline struct fields (Issue #6693). Composite keys
/// must be heap-free before hashing, or a heap index would leak into the key.
fn composite_value_has_structref(value: &Value) -> bool {
    match value {
        Value::StructRef(_) => true,
        Value::Struct(inst) => inst.values.iter().any(composite_value_has_structref),
        Value::Tuple(t) | Value::SimpleVector(t) => {
            t.elements.iter().any(composite_value_has_structref)
        }
        Value::NamedTuple(nt) => nt.values.iter().any(composite_value_has_structref),
        _ => false,
    }
}

/// Canonical Dict/Set key payload for type objects (Issue #5108).
///
/// Equality and hashing use only `canonical`; `original` is retained so the
/// key can be projected back to the original type-object `Value`.
#[derive(Debug, Clone)]
pub struct TypeDictKey {
    canonical: u64,
    original: Box<Value>,
}

// Identity is the canonical structural hash; `original` is carried only for
// projection back to a `Value` and never participates in equality (Issue #5108).
impl PartialEq for TypeDictKey {
    fn eq(&self, other: &Self) -> bool {
        self.canonical == other.canonical
    }
}

impl Eq for TypeDictKey {}

impl TypeDictKey {
    /// Compute the canonical, registry-free structural hash for a type-object
    /// `Value` (`DataType` or fresh `TypeVar`). Returns `None` for non-type
    /// values so callers can fall through to the invalid-key path.
    fn from_type_value(value: &Value) -> Option<Self> {
        use crate::inference_core::CoreType;

        let mut hasher = DefaultHasher::new();
        match value {
            Value::DataType(jt) => {
                // The shared CoreType lowering canonicalizes alternate
                // `JuliaType` representations of the same semantic type, so it
                // is the structural identity used by `objectid`/`hash` too.
                "DataType".hash(&mut hasher);
                CoreType::from(jt.as_ref()).hash(&mut hasher);
            }
            Value::RuntimeTypeVar(tv) => {
                // Fresh TypeVars carry identity in upstream Julia: two distinct
                // `TypeVar`s with the same name are not equal. Key on the
                // identity id to preserve that.
                "TypeVar".hash(&mut hasher);
                tv.id.hash(&mut hasher);
            }
            _ => return None,
        }
        Some(Self {
            canonical: hasher.finish(),
            original: Box::new(value.clone()),
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum NumericDictKey {
    Signed(i128),
    Unsigned(u128),
    Float(FloatDictKey),
}

#[derive(Debug, Clone, Copy)]
enum FloatDictKey {
    F16(u16),
    F32(u32),
    F64(u64),
}

impl DictKey {
    pub fn from_value(v: &Value) -> Result<Self, VmError> {
        match v {
            Value::Str(s) => Ok(DictKey::Str(s.clone())),
            Value::F16(f) => Ok(DictKey::F16(f.to_bits())),
            Value::F32(f) => Ok(DictKey::F32(f.to_bits())),
            Value::F64(f) => Ok(DictKey::F64(f.to_bits())),
            Value::I64(i) => Ok(DictKey::I64(*i)),
            Value::I32(i) => Ok(DictKey::I32(*i)),
            Value::I16(i) => Ok(DictKey::I16(*i)),
            Value::I8(i) => Ok(DictKey::I8(*i)),
            Value::I128(i) => Ok(DictKey::I128(*i)),
            Value::U8(i) => Ok(DictKey::U8(*i)),
            Value::U16(i) => Ok(DictKey::U16(*i)),
            Value::U32(i) => Ok(DictKey::U32(*i)),
            Value::U64(i) => Ok(DictKey::U64(*i)),
            Value::U128(i) => Ok(DictKey::U128(*i)),
            Value::Symbol(sym) => Ok(DictKey::Symbol(sym.as_str().to_string())),
            // Type objects as keys (Issue #5108): DataType / fresh TypeVar.
            Value::DataType(_) | Value::RuntimeTypeVar(_) => TypeDictKey::from_type_value(v)
                .map(DictKey::Type)
                .ok_or_else(|| VmError::InvalidDictKey(format!("{:?}", v))),
            // Composite keys: tuples / named tuples / svecs / structs (Issue
            // #6693). The value must be heap-free; callers resolve `StructRef`s
            // first (so `(OneTo(3),)` collapses to one key).
            Value::Tuple(_) | Value::SimpleVector(_) | Value::NamedTuple(_) | Value::Struct(_) => {
                CompositeDictKey::from_value(v)
                    .map(DictKey::Composite)
                    .ok_or_else(|| VmError::InvalidDictKey(format!("{:?}", v)))
            }
            _ => Err(VmError::InvalidDictKey(format!("{:?}", v))),
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            DictKey::Str(s) => Value::Str(s.clone()),
            DictKey::F16(bits) => Value::F16(f16::from_bits(*bits)),
            DictKey::F32(bits) => Value::F32(f32::from_bits(*bits)),
            DictKey::F64(bits) => Value::F64(f64::from_bits(*bits)),
            DictKey::I64(i) => Value::I64(*i),
            DictKey::I32(i) => Value::I32(*i),
            DictKey::I16(i) => Value::I16(*i),
            DictKey::I8(i) => Value::I8(*i),
            DictKey::I128(i) => Value::I128(*i),
            DictKey::U8(i) => Value::U8(*i),
            DictKey::U16(i) => Value::U16(*i),
            DictKey::U32(i) => Value::U32(*i),
            DictKey::U64(i) => Value::U64(*i),
            DictKey::U128(i) => Value::U128(*i),
            DictKey::Symbol(s) => Value::Symbol(super::macro_::SymbolValue::new(s.clone())),
            // Type-object keys round-trip to the original type value (Issue #5108).
            DictKey::Type(t) => (*t.original).clone(),
            // Composite keys round-trip to the original value (Issue #6693).
            DictKey::Composite(c) => (*c.original).clone(),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            DictKey::Str(_) => "String",
            DictKey::F16(_) => "Float16",
            DictKey::F32(_) => "Float32",
            DictKey::F64(_) => "Float64",
            DictKey::I64(_) => "Int64",
            DictKey::I32(_) => "Int32",
            DictKey::I16(_) => "Int16",
            DictKey::I8(_) => "Int8",
            DictKey::I128(_) => "Int128",
            DictKey::U8(_) => "UInt8",
            DictKey::U16(_) => "UInt16",
            DictKey::U32(_) => "UInt32",
            DictKey::U64(_) => "UInt64",
            DictKey::U128(_) => "UInt128",
            DictKey::Symbol(_) => "Symbol",
            // Type objects: typeof is DataType (Issue #5108).
            DictKey::Type(_) => "DataType",
            // Composite keys (Issue #6693). A static label per kind — the exact
            // parametric element type (e.g. `Tuple{Int64,Int64}`) is not
            // recovered here, so a Set of such keys may display its element type
            // less precisely than upstream.
            DictKey::Composite(c) => match c.original.as_ref() {
                Value::Tuple(_) => "Tuple",
                Value::NamedTuple(_) => "NamedTuple",
                Value::SimpleVector(_) => "Core.SimpleVector",
                _ => "Any",
            },
        }
    }

    pub fn as_keyword_name(&self) -> Option<&str> {
        match self {
            DictKey::Str(s) | DictKey::Symbol(s) => Some(s),
            _ => None,
        }
    }

    pub fn matches_value(&self, value: &Value) -> bool {
        if let Ok(key) = DictKey::from_value(value) {
            return self == &key;
        }
        match (value, self.numeric_value()) {
            (Value::F64(n), Some(numeric)) => numeric.to_f64() == *n,
            _ => false,
        }
    }

    fn numeric_value(&self) -> Option<NumericDictKey> {
        match self {
            DictKey::I64(i) => Some(NumericDictKey::Signed(i128::from(*i))),
            DictKey::I32(i) => Some(NumericDictKey::Signed(i128::from(*i))),
            DictKey::I16(i) => Some(NumericDictKey::Signed(i128::from(*i))),
            DictKey::I8(i) => Some(NumericDictKey::Signed(i128::from(*i))),
            DictKey::I128(i) => Some(NumericDictKey::Signed(*i)),
            DictKey::U8(i) => Some(NumericDictKey::Unsigned(u128::from(*i))),
            DictKey::U16(i) => Some(NumericDictKey::Unsigned(u128::from(*i))),
            DictKey::U32(i) => Some(NumericDictKey::Unsigned(u128::from(*i))),
            DictKey::U64(i) => Some(NumericDictKey::Unsigned(u128::from(*i))),
            DictKey::U128(i) => Some(NumericDictKey::Unsigned(*i)),
            DictKey::F16(bits) => Some(NumericDictKey::Float(FloatDictKey::F16(*bits))),
            DictKey::F32(bits) => Some(NumericDictKey::Float(FloatDictKey::F32(*bits))),
            DictKey::F64(bits) => Some(NumericDictKey::Float(FloatDictKey::F64(*bits))),
            DictKey::Str(_) | DictKey::Symbol(_) | DictKey::Type(_) | DictKey::Composite(_) => None,
        }
    }
}

impl NumericDictKey {
    fn equals(self, other: Self) -> bool {
        match (self, other) {
            (NumericDictKey::Signed(a), NumericDictKey::Signed(b)) => a == b,
            (NumericDictKey::Unsigned(a), NumericDictKey::Unsigned(b)) => a == b,
            (NumericDictKey::Signed(a), NumericDictKey::Unsigned(b)) => {
                a >= 0 && u128::try_from(a).ok() == Some(b)
            }
            (NumericDictKey::Unsigned(a), NumericDictKey::Signed(b)) => {
                b >= 0 && u128::try_from(b).ok() == Some(a)
            }
            (NumericDictKey::Float(a), NumericDictKey::Float(b)) => a.isequal(b),
            (NumericDictKey::Signed(a), NumericDictKey::Float(b))
            | (NumericDictKey::Float(b), NumericDictKey::Signed(a)) => b
                .integral_value()
                .is_some_and(|float_int| NumericDictKey::Signed(a).equals(float_int)),
            (NumericDictKey::Unsigned(a), NumericDictKey::Float(b))
            | (NumericDictKey::Float(b), NumericDictKey::Unsigned(a)) => b
                .integral_value()
                .is_some_and(|float_int| NumericDictKey::Unsigned(a).equals(float_int)),
        }
    }

    fn hash_into<H: Hasher>(self, state: &mut H) {
        match self {
            NumericDictKey::Signed(n) if n >= 0 => {
                0u8.hash(state);
                (n as u128).hash(state);
            }
            NumericDictKey::Signed(n) => {
                1u8.hash(state);
                n.hash(state);
            }
            NumericDictKey::Unsigned(n) if i128::try_from(n).is_ok() => {
                0u8.hash(state);
                n.hash(state);
            }
            NumericDictKey::Unsigned(n) => {
                2u8.hash(state);
                n.hash(state);
            }
            NumericDictKey::Float(n) => n.hash_into(state),
        }
    }

    fn to_f64(self) -> f64 {
        match self {
            NumericDictKey::Signed(n) => n as f64,
            NumericDictKey::Unsigned(n) => n as f64,
            NumericDictKey::Float(n) => n.to_f64(),
        }
    }
}

impl FloatDictKey {
    fn to_f64(self) -> f64 {
        match self {
            FloatDictKey::F16(bits) => f64::from(f16::from_bits(bits).to_f32()),
            FloatDictKey::F32(bits) => f64::from(f32::from_bits(bits)),
            FloatDictKey::F64(bits) => f64::from_bits(bits),
        }
    }

    fn is_negative_zero(self) -> bool {
        match self {
            FloatDictKey::F16(bits) => bits == f16::NEG_ZERO.to_bits(),
            FloatDictKey::F32(bits) => bits == (-0.0f32).to_bits(),
            FloatDictKey::F64(bits) => bits == (-0.0f64).to_bits(),
        }
    }

    fn isequal(self, other: Self) -> bool {
        let left = self.to_f64();
        let right = other.to_f64();
        if left.is_nan() && right.is_nan() {
            return true;
        }
        if left == 0.0 && right == 0.0 {
            return self.is_negative_zero() == other.is_negative_zero();
        }
        left == right
    }

    fn integral_value(self) -> Option<NumericDictKey> {
        let value = self.to_f64();
        if value.is_nan() || !value.is_finite() || self.is_negative_zero() {
            return None;
        }
        if value.fract() != 0.0 {
            return None;
        }
        if value < 0.0 {
            if value < i128::MIN as f64 {
                return None;
            }
            Some(NumericDictKey::Signed(value as i128))
        } else {
            if value > u128::MAX as f64 {
                return None;
            }
            Some(NumericDictKey::Unsigned(value as u128))
        }
    }

    fn hash_into<H: Hasher>(self, state: &mut H) {
        if let Some(integer) = self.integral_value() {
            integer.hash_into(state);
            return;
        }
        if self.to_f64().is_nan() {
            5u8.hash(state);
            return;
        }
        6u8.hash(state);
        self.is_negative_zero().hash(state);
        self.to_f64().to_bits().hash(state);
    }
}

/// Borrowed, non-numeric "shape" of a dict key shared by the owned [`DictKey`]
/// and the borrowed [`KeyRef`] probe path (Issue #5187).
///
/// Numeric keys are handled separately via [`NumericDictKey`]; this only covers
/// the variants whose owned form would otherwise allocate (`Str`/`Symbol`) plus
/// the canonical type-object hash. Routing both the owned and borrowed paths
/// through [`hash_key_shape`] guarantees byte-identical `DefaultHasher` output
/// so a borrowed probe lands in exactly the same bucket as the owned key.
enum KeyShape<'a> {
    Str(&'a str),
    Symbol(&'a str),
    Type(u64),
    Composite(u64),
}

/// Hash a [`KeyShape`] using the exact discriminant + payload scheme that the
/// owned `DictKey` hash used before Issue #5187, preserving on-disk/in-table
/// probe behaviour.
#[inline]
fn hash_key_shape<H: Hasher>(shape: &KeyShape<'_>, state: &mut H) {
    match shape {
        KeyShape::Str(s) => {
            3u8.hash(state);
            s.hash(state);
        }
        KeyShape::Symbol(s) => {
            4u8.hash(state);
            s.hash(state);
        }
        // Type-object keys hash on their canonical structural hash so equal
        // types land in the same bucket (Issue #5108).
        KeyShape::Type(canonical) => {
            5u8.hash(state);
            canonical.hash(state);
        }
        // Composite keys hash on their canonical structural hash (Issue #6693).
        KeyShape::Composite(canonical) => {
            6u8.hash(state);
            canonical.hash(state);
        }
    }
}

impl DictKey {
    /// The non-numeric [`KeyShape`] for this key, or `None` for numeric keys
    /// (which hash/compare via [`NumericDictKey`]).
    #[inline]
    fn key_shape(&self) -> Option<KeyShape<'_>> {
        match self {
            DictKey::Str(s) => Some(KeyShape::Str(s)),
            DictKey::Symbol(s) => Some(KeyShape::Symbol(s)),
            DictKey::Type(t) => Some(KeyShape::Type(t.canonical)),
            DictKey::Composite(c) => Some(KeyShape::Composite(c.canonical)),
            _ => None,
        }
    }
}

impl PartialEq for DictKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DictKey::Str(a), DictKey::Str(b)) => a == b,
            (DictKey::Symbol(a), DictKey::Symbol(b)) => a == b,
            // Type-object keys: equal iff their canonical structural hashes
            // match (Issue #5108). Never equal to a non-type key.
            (DictKey::Type(a), DictKey::Type(b)) => a.canonical == b.canonical,
            (DictKey::Type(_), _) | (_, DictKey::Type(_)) => false,
            // Composite keys compare structurally (Issue #6693). Must precede the
            // numeric fallback: `numeric_value()` is `None` for composites, so
            // without this two equal composites would wrongly compare `false`.
            (DictKey::Composite(a), DictKey::Composite(b)) => a == b,
            (DictKey::Composite(_), _) | (_, DictKey::Composite(_)) => false,
            _ => match (self.numeric_value(), other.numeric_value()) {
                (Some(a), Some(b)) => a.equals(b),
                _ => false,
            },
        }
    }
}

impl Hash for DictKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if let Some(numeric) = self.numeric_value() {
            numeric.hash_into(state);
            return;
        }
        if let Some(shape) = self.key_shape() {
            hash_key_shape(&shape, state);
        }
    }
}

/// Borrowed dict-key probe that mirrors [`DictKey`] without allocating an owned
/// key for string/symbol reads (Issue #5187).
///
/// `Str`/`Symbol` borrow a `&str` straight out of the `Value` being looked up,
/// so a `Dict{String,V}[s]` read no longer clones `s`. Numeric and type keys
/// reuse the same owned representation (they are already cheap / already need an
/// allocation to carry the canonical hash), so the probe is exact for every key
/// flavor. Hashing and equality are shared with `DictKey` via [`KeyShape`] /
/// [`NumericDictKey`], so a `KeyRef` always lands in the same probe bucket as
/// the owned key it represents.
enum KeyRef<'a> {
    Str(&'a str),
    Symbol(&'a str),
    Numeric(NumericDictKey),
    Type(TypeDictKey),
}

/// Compute the `FxHasher` hash of a [`KeyRef`], matching [`hash_dict_key`].
///
/// Issue #5188 switched the internal dict-slot hash from `DefaultHasher` to
/// `FxHasher`; the borrowed probe (Issue #5187) MUST use the same hasher, or it
/// would compute a different slot than the owned insert path and silently miss.
#[inline]
fn hash_key_ref(key: &KeyRef<'_>) -> u64 {
    let mut hasher = FxHasher::default();
    key.hash_into(&mut hasher);
    hasher.finish()
}

impl<'a> KeyRef<'a> {
    /// Build a borrowed probe for `value`, or `None` if `value` is not a valid
    /// dict key (mirrors `DictKey::from_value(..).is_err()`).
    fn from_value(value: &'a Value) -> Option<Self> {
        match value {
            Value::Str(s) => Some(KeyRef::Str(s)),
            Value::Symbol(sym) => Some(KeyRef::Symbol(sym.as_str())),
            Value::F16(f) => Some(KeyRef::Numeric(NumericDictKey::Float(FloatDictKey::F16(
                f.to_bits(),
            )))),
            Value::F32(f) => Some(KeyRef::Numeric(NumericDictKey::Float(FloatDictKey::F32(
                f.to_bits(),
            )))),
            Value::F64(f) => Some(KeyRef::Numeric(NumericDictKey::Float(FloatDictKey::F64(
                f.to_bits(),
            )))),
            Value::I64(i) => Some(KeyRef::Numeric(NumericDictKey::Signed(i128::from(*i)))),
            Value::I32(i) => Some(KeyRef::Numeric(NumericDictKey::Signed(i128::from(*i)))),
            Value::I16(i) => Some(KeyRef::Numeric(NumericDictKey::Signed(i128::from(*i)))),
            Value::I8(i) => Some(KeyRef::Numeric(NumericDictKey::Signed(i128::from(*i)))),
            Value::I128(i) => Some(KeyRef::Numeric(NumericDictKey::Signed(*i))),
            Value::U8(i) => Some(KeyRef::Numeric(NumericDictKey::Unsigned(u128::from(*i)))),
            Value::U16(i) => Some(KeyRef::Numeric(NumericDictKey::Unsigned(u128::from(*i)))),
            Value::U32(i) => Some(KeyRef::Numeric(NumericDictKey::Unsigned(u128::from(*i)))),
            Value::U64(i) => Some(KeyRef::Numeric(NumericDictKey::Unsigned(u128::from(*i)))),
            Value::U128(i) => Some(KeyRef::Numeric(NumericDictKey::Unsigned(*i))),
            // Type objects: build the canonical key (this allocates only the
            // `original` Box, exactly as the owned path does — Issue #5108).
            Value::DataType(_) | Value::RuntimeTypeVar(_) => {
                TypeDictKey::from_type_value(value).map(KeyRef::Type)
            }
            _ => None,
        }
    }

    /// Hash the probe with the same scheme as the owned [`DictKey`].
    #[inline]
    fn hash_into<H: Hasher>(&self, state: &mut H) {
        match self {
            KeyRef::Str(s) => hash_key_shape(&KeyShape::Str(s), state),
            KeyRef::Symbol(s) => hash_key_shape(&KeyShape::Symbol(s), state),
            KeyRef::Type(t) => hash_key_shape(&KeyShape::Type(t.canonical), state),
            KeyRef::Numeric(n) => n.hash_into(state),
        }
    }

    /// Equality against an owned [`DictKey`], matching `DictKey`'s `PartialEq`
    /// (including cross-numeric-width comparisons).
    #[inline]
    fn matches_dict_key(&self, key: &DictKey) -> bool {
        match self {
            KeyRef::Str(s) => matches!(key, DictKey::Str(k) if k == s),
            KeyRef::Symbol(s) => matches!(key, DictKey::Symbol(k) if k == s),
            KeyRef::Type(t) => matches!(key, DictKey::Type(k) if k.canonical == t.canonical),
            KeyRef::Numeric(n) => key.numeric_value().is_some_and(|k| n.equals(k)),
        }
    }
}

impl std::fmt::Display for DictKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DictKey::Str(s) => write!(f, "\"{}\"", s),
            DictKey::F16(bits) => write!(f, "{}", f16::from_bits(*bits)),
            DictKey::F32(bits) => write!(f, "{}", f32::from_bits(*bits)),
            DictKey::F64(bits) => write!(f, "{}", f64::from_bits(*bits)),
            DictKey::I64(i) => write!(f, "{}", i),
            DictKey::I32(i) => write!(f, "{}", i),
            DictKey::I16(i) => write!(f, "{}", i),
            DictKey::I8(i) => write!(f, "{}", i),
            DictKey::I128(i) => write!(f, "{}", i),
            DictKey::U8(i) => write!(f, "{}", i),
            DictKey::U16(i) => write!(f, "{}", i),
            DictKey::U32(i) => write!(f, "{}", i),
            DictKey::U64(i) => write!(f, "{}", i),
            DictKey::U128(i) => write!(f, "{}", i),
            DictKey::Symbol(s) => write!(f, ":{}", s),
            // Render the original type object (Issue #5108).
            DictKey::Type(t) => write!(f, "{:?}", t.original),
            // Render the original composite value (Issue #6693). User-facing
            // Set/Dict display projects keys to a `Value` and formats those (see
            // vm::formatting); this `Display` is a heap-less fallback.
            DictKey::Composite(c) => write!(f, "{:?}", c.original),
        }
    }
}

// Hash table constants matching Julia's dict.jl
const SLOT_EMPTY: u8 = 0x00;
const SLOT_DELETED: u8 = 0x7f;
const MIN_DICT_TABLE_SIZE: usize = 16;
const MAX_ALLOWED_PROBE: usize = 16;
const MAX_PROBE_SHIFT: usize = 6;

/// Get the 7 most significant bits of the hash, with high bit set.
#[inline]
fn shorthash7(hsh: u64) -> u8 {
    ((hsh >> 57) as u8) | 0x80
}

/// A small, fast, non-cryptographic hasher used only for internal dict-key
/// slot placement (Issue #5188).
///
/// This is a hand-rolled FxHash (the algorithm `rustc` and `rustc-hash` use):
/// `hash = (hash.rotate_left(5) ^ word).wrapping_mul(K)` per consumed word.
/// It replaces `std`'s `DefaultHasher` (SipHash-1-3), which is a keyed,
/// crypto-grade PRF and far slower than necessary here.
///
/// Crucially, slot positions in the open-addressing dict are an *internal*
/// implementation detail and are never observable from Julia, so the exact
/// digest carries no upstream-compatibility constraint. The user-facing
/// `hash()`/`objectid` path and the `TypeDictKey` canonical hash are
/// deliberately left untouched (they remain on `DefaultHasher`).
///
/// `DictKey`'s `impl Hash` still writes into this generic `Hasher`, so the
/// `Hash`/`PartialEq` consistency for numeric (NaN/-0.0), `Str`, `Symbol`, and
/// canonical `Type` keys is preserved automatically.
#[derive(Default)]
struct FxHasher {
    hash: u64,
}

impl FxHasher {
    /// FxHash 64-bit multiplier (golden-ratio-derived odd constant).
    const K: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    #[inline]
    fn add_word(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(Self::K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        // Consume 8 bytes at a time, then fold the tail. This mirrors the
        // word-at-a-time behaviour of rustc-hash without depending on it.
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            // `chunks_exact(8)` always yields 8-byte slices, so this is
            // infallible; use `copy_from_slice` (like the remainder branch
            // below) instead of a fallible slice-to-array conversion, keeping
            // the VM panic-free audit (vm_unwrap_count_does_not_regress,
            // Issue #2193) at baseline 0.
            let mut buf = [0u8; 8];
            buf.copy_from_slice(chunk);
            self.add_word(u64::from_le_bytes(buf));
        }
        let remainder = chunks.remainder();
        if !remainder.is_empty() {
            let mut buf = [0u8; 8];
            buf[..remainder.len()].copy_from_slice(remainder);
            self.add_word(u64::from_le_bytes(buf));
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add_word(u64::from(i));
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add_word(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add_word(i as u64);
    }
}

/// Compute hash of a DictKey using the internal fast non-crypto hasher.
#[inline]
fn hash_dict_key(key: &DictKey) -> u64 {
    let mut hasher = FxHasher::default();
    key.hash(&mut hasher);
    hasher.finish()
}

/// Compute the optimal slot position and short hash for a key.
/// Returns (0-based index, shorthash7).
#[inline]
fn hashindex(key: &DictKey, sz: usize) -> (usize, u8) {
    let hsh = hash_dict_key(key);
    let idx = (hsh as usize) & (sz - 1);
    (idx, shorthash7(hsh))
}

/// Round up to next power of 2, with a minimum table size.
/// Matches Julia's _tablesz function.
fn table_size(x: usize) -> usize {
    if x == 0 {
        return 0;
    }
    let min_sz = if x < MIN_DICT_TABLE_SIZE {
        MIN_DICT_TABLE_SIZE
    } else {
        x
    };
    min_sz.next_power_of_two()
}

#[inline]
fn is_slot_empty(slot: u8) -> bool {
    slot == SLOT_EMPTY
}

#[inline]
fn is_slot_filled(slot: u8) -> bool {
    (slot & 0x80) != 0
}

#[inline]
fn is_slot_deleted(slot: u8) -> bool {
    slot == SLOT_DELETED
}

/// Dictionary value: key-value mapping (open-addressing hash table).
///
/// Internal storage matches Julia's Dict struct layout:
/// - `slots`: Memory{UInt8} — slot metadata (0x00=empty, 0x7f=deleted, 0x80|sh=filled)
/// - `keys`: Memory{K} — key storage
/// - `vals`: Memory{V} — value storage
///
/// Uses open addressing with linear probing, matching Julia's hash table algorithm.
#[derive(Debug, Clone)]
pub struct DictValue {
    /// Slot metadata: 0x00=empty, 0x7f=deleted, 0x80|shorthash7=filled
    slots: Vec<u8>,
    /// Key storage (valid only when corresponding slot is filled)
    keys: Vec<DictKey>,
    /// Value storage (valid only when corresponding slot is filled)
    vals: Vec<Value>,
    /// Number of deleted entries
    ndel: usize,
    /// Number of live entries
    count: usize,
    /// Maximum probe distance
    maxprobe: usize,
    /// Type parameter for keys (e.g., "Int64" for Dict{Int64,V})
    pub key_type: Option<String>,
    /// Type parameter for values (e.g., "String" for Dict{K,String})
    pub value_type: Option<String>,
}

/// Iterator over filled entries of a DictValue.
pub struct DictIter<'a> {
    dict: &'a DictValue,
    index: usize,
}

impl<'a> Iterator for DictIter<'a> {
    type Item = (&'a DictKey, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.dict.slots.len() {
            let i = self.index;
            self.index += 1;
            if is_slot_filled(self.dict.slots[i]) {
                return Some((&self.dict.keys[i], &self.dict.vals[i]));
            }
        }
        None
    }
}

impl DictValue {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            keys: Vec::new(),
            vals: Vec::new(),
            ndel: 0,
            count: 0,
            maxprobe: 0,
            key_type: None,
            value_type: None,
        }
    }

    pub fn with_entries(entries: Vec<(DictKey, Value)>) -> Self {
        let mut dict = Self::new();
        for (k, v) in entries {
            dict.insert(k, v);
        }
        dict
    }

    pub fn with_type_params(key_type: String, value_type: String) -> Self {
        Self {
            slots: Vec::new(),
            keys: Vec::new(),
            vals: Vec::new(),
            ndel: 0,
            count: 0,
            maxprobe: 0,
            key_type: Some(key_type),
            value_type: Some(value_type),
        }
    }

    /// Create a DictValue with optional type params.
    pub fn with_type_params_opt(key_type: Option<String>, value_type: Option<String>) -> Self {
        Self {
            slots: Vec::new(),
            keys: Vec::new(),
            vals: Vec::new(),
            ndel: 0,
            count: 0,
            maxprobe: 0,
            key_type,
            value_type,
        }
    }

    /// Get value by key. Returns None if key not found.
    pub fn get(&self, key: &DictKey) -> Option<&Value> {
        self.ht_keyindex(key).map(|idx| &self.vals[idx])
    }

    /// Get value by a `Value` key without allocating an owned `DictKey` for
    /// string/symbol reads (Issue #5187). Returns `None` if `value` is not a
    /// valid key or is absent — same observable result as
    /// `DictKey::from_value(value).ok().and_then(|k| self.get(&k))`, but the
    /// borrowed probe hashes/compares the `&str` directly against stored keys.
    pub fn get_by_value(&self, value: &Value) -> Option<&Value> {
        let probe = KeyRef::from_value(value)?;
        self.ht_keyindex_ref(&probe).map(|idx| &self.vals[idx])
    }

    /// Check whether `value` is a key, using the borrowed probe (Issue #5187).
    pub fn contains_key_by_value(&self, value: &Value) -> bool {
        match KeyRef::from_value(value) {
            Some(probe) => self.ht_keyindex_ref(&probe).is_some(),
            None => false,
        }
    }

    /// Borrowed read that distinguishes an unsupported key type from a missing
    /// key (Issue #5187), without cloning the key on the hot (hit) path.
    ///
    /// - `Err(VmError::InvalidDictKey)` — `value` is not a valid Dict key type
    ///   (byte-for-byte the same error `DictKey::from_value` would raise).
    /// - `Ok(None)` — valid key type, but absent.
    /// - `Ok(Some(v))` — found.
    ///
    /// Callers that need the two distinct downstream messages (`TypeError` vs
    /// `DictKeyNotFound`) can branch on this without ever building an owned
    /// `DictKey` for a successful lookup.
    pub fn get_by_value_checked(&self, value: &Value) -> Result<Option<&Value>, VmError> {
        match KeyRef::from_value(value) {
            None => Err(VmError::InvalidDictKey(format!("{:?}", value))),
            Some(probe) => Ok(self.ht_keyindex_ref(&probe).map(|idx| &self.vals[idx])),
        }
    }

    /// Insert a key-value pair. Updates the value if key already exists.
    pub fn insert(&mut self, key: DictKey, value: Value) {
        let (pos, sh) = self.ht_keyindex2(&key);
        if pos >= 0 {
            // Key exists, update value
            self.vals[pos as usize] = value;
        } else {
            // Key not found, insert at (-pos - 1)
            let idx = (-(pos + 1)) as usize;
            if is_slot_deleted(self.slots[idx]) {
                self.ndel -= 1;
            }
            self.slots[idx] = sh;
            self.keys[idx] = key;
            self.vals[idx] = value;
            self.count += 1;

            // Rehash now if necessary (matching Julia's _setindex! logic):
            // > 3/4 deleted or > 2/3 full
            let sz = self.slots.len();
            if self.ndel >= (3 * sz / 4) || self.count * 3 > sz * 2 {
                let new_cnt = self.count;
                let new_sz = if new_cnt > 64000 {
                    new_cnt * 2
                } else {
                    new_cnt * 4
                };
                self.rehash(new_sz);
            }
        }
    }

    /// Remove a key and return its value, or None if key not found.
    pub fn remove(&mut self, key: &DictKey) -> Option<Value> {
        if let Some(idx) = self.ht_keyindex(key) {
            let val = std::mem::replace(&mut self.vals[idx], Value::Nothing);
            self.slots[idx] = SLOT_DELETED;
            self.ndel += 1;
            self.count -= 1;
            Some(val)
        } else {
            None
        }
    }

    /// Check if the dict contains the given key.
    pub fn contains_key(&self, key: &DictKey) -> bool {
        self.ht_keyindex(key).is_some()
    }

    /// Get the number of live entries.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Check if the dict is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Get all keys as a Vec.
    pub fn keys(&self) -> Vec<DictKey> {
        self.iter().map(|(k, _)| k.clone()).collect()
    }

    /// Get all values as a Vec.
    pub fn values(&self) -> Vec<Value> {
        self.iter().map(|(_, v)| v.clone()).collect()
    }

    /// Iterate over filled entries as (&DictKey, &Value) pairs.
    pub fn iter(&self) -> DictIter<'_> {
        DictIter {
            dict: self,
            index: 0,
        }
    }

    /// Find the next filled slot starting from `from_index` (0-based).
    /// Returns (slot_index, &key, &value) or None if no more filled slots.
    pub fn next_filled_slot(&self, from_index: usize) -> Option<(usize, &DictKey, &Value)> {
        let sz = self.slots.len();
        let mut i = from_index;
        while i < sz {
            if is_slot_filled(self.slots[i]) {
                return Some((i, &self.keys[i], &self.vals[i]));
            }
            i += 1;
        }
        None
    }

    /// Merge another dict into this one (other's values override).
    pub fn merge(&mut self, other: &DictValue) {
        for (k, v) in other.iter() {
            self.insert(k.clone(), v.clone());
        }
    }

    /// Clear all entries (keeps allocated capacity).
    pub fn clear(&mut self) {
        let sz = self.slots.len();
        self.slots.fill(SLOT_EMPTY);
        for i in 0..sz {
            self.keys[i] = DictKey::I64(0);
            self.vals[i] = Value::Nothing;
        }
        self.ndel = 0;
        self.count = 0;
        self.maxprobe = 0;
    }

    /// Get a stable identity value for hashing (used by objectid).
    pub fn identity_ptr(&self) -> usize {
        self.slots.as_ptr() as usize
    }

    // =========================================================================
    // Internal hash table operations
    // =========================================================================

    /// Find the slot index where a key is stored, or None if not present.
    fn ht_keyindex(&self, key: &DictKey) -> Option<usize> {
        let sz = self.slots.len();
        if sz == 0 {
            return None;
        }
        let (mut index, sh) = hashindex(key, sz);
        let mut iter = 0;
        loop {
            if is_slot_empty(self.slots[index]) {
                return None;
            }
            if self.slots[index] == sh && self.keys[index] == *key {
                return Some(index);
            }
            index = (index + 1) & (sz - 1);
            iter += 1;
            if iter > self.maxprobe {
                return None;
            }
        }
    }

    /// Borrowed-probe twin of [`ht_keyindex`] (Issue #5187). Identical probe
    /// sequence — same `hashindex`/`shorthash7`/`maxprobe` semantics — but hashes
    /// and compares a [`KeyRef`] against the stored owned keys so no owned
    /// `DictKey` is allocated for string/symbol reads.
    fn ht_keyindex_ref(&self, key: &KeyRef<'_>) -> Option<usize> {
        let sz = self.slots.len();
        if sz == 0 {
            return None;
        }
        let hsh = hash_key_ref(key);
        let mut index = (hsh as usize) & (sz - 1);
        let sh = shorthash7(hsh);
        let mut iter = 0;
        loop {
            if is_slot_empty(self.slots[index]) {
                return None;
            }
            if self.slots[index] == sh && key.matches_dict_key(&self.keys[index]) {
                return Some(index);
            }
            index = (index + 1) & (sz - 1);
            iter += 1;
            if iter > self.maxprobe {
                return None;
            }
        }
    }

    /// Find the position for inserting a key.
    /// Returns (pos, shorthash) where:
    /// - pos >= 0: key exists at this position
    /// - pos < 0: key not found, insert at (-pos - 1)
    fn ht_keyindex2(&mut self, key: &DictKey) -> (isize, u8) {
        let sz = self.slots.len();
        if sz == 0 {
            self.rehash(4);
            let (idx, sh) = hashindex(key, self.slots.len());
            return (-(idx as isize) - 1, sh);
        }
        let (start_index, sh) = hashindex(key, sz);
        let mut index = start_index;
        let mut avail: isize = 0;
        let mut iter = 0;

        loop {
            if is_slot_empty(self.slots[index]) {
                let pos = if avail < 0 {
                    (-avail - 1) as usize
                } else {
                    index
                };
                return (-(pos as isize) - 1, sh);
            }
            if is_slot_deleted(self.slots[index]) {
                if avail == 0 {
                    avail = -(index as isize) - 1;
                }
            } else if self.slots[index] == sh && self.keys[index] == *key {
                return (index as isize, sh);
            }
            index = (index + 1) & (sz - 1);
            iter += 1;
            if iter > self.maxprobe {
                break;
            }
        }

        if avail < 0 {
            return (avail, sh);
        }

        let max_allowed = std::cmp::max(MAX_ALLOWED_PROBE, sz >> MAX_PROBE_SHIFT);
        while iter < max_allowed {
            if !is_slot_filled(self.slots[index]) {
                self.maxprobe = iter;
                return (-(index as isize) - 1, sh);
            }
            index = (index + 1) & (sz - 1);
            iter += 1;
        }

        // Need to rehash and retry
        let new_sz = if self.count > 64000 { sz * 2 } else { sz * 4 };
        self.rehash(new_sz);
        self.ht_keyindex2(key)
    }

    /// Rehash the hash table to a new size.
    fn rehash(&mut self, newsz: usize) {
        let newsz = table_size(newsz);
        if self.count == 0 {
            self.slots = vec![SLOT_EMPTY; newsz];
            self.keys = vec![DictKey::I64(0); newsz];
            self.vals = vec![Value::Nothing; newsz];
            self.ndel = 0;
            self.maxprobe = 0;
            return;
        }

        let old_slots = std::mem::replace(&mut self.slots, vec![SLOT_EMPTY; newsz]);
        let mut old_keys = std::mem::replace(&mut self.keys, vec![DictKey::I64(0); newsz]);
        let mut old_vals = std::mem::replace(&mut self.vals, vec![Value::Nothing; newsz]);

        let mut count = 0;
        let mut maxprobe = 0;

        for i in 0..old_slots.len() {
            if is_slot_filled(old_slots[i]) {
                let key = std::mem::replace(&mut old_keys[i], DictKey::I64(0));
                let val = std::mem::replace(&mut old_vals[i], Value::Nothing);

                let (mut index, _) = hashindex(&key, newsz);
                let index0 = index;
                while self.slots[index] != SLOT_EMPTY {
                    index = (index + 1) & (newsz - 1);
                }
                let probe = index.wrapping_sub(index0) & (newsz - 1);
                if probe > maxprobe {
                    maxprobe = probe;
                }
                self.slots[index] = old_slots[i];
                self.keys[index] = key;
                self.vals[index] = val;
                count += 1;
            }
        }

        self.count = count;
        self.ndel = 0;
        self.maxprobe = maxprobe;
    }
}

impl Default for DictValue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `SetValue` moved to its sibling `set` module in the #6835 split; one Dict
    // test constructs an empty `SetValue` to contrast empty-collection behavior.
    use crate::vm::value::SetValue;

    #[test]
    fn test_dict_new_empty() {
        let d = DictValue::new();
        assert_eq!(d.len(), 0);
        assert!(d.is_empty());
        assert!(d.get(&DictKey::Str("x".into())).is_none());
    }

    #[test]
    fn composite_dict_keys_compare_and_hash_by_structure_6693() {
        use crate::vm::value::TupleValue;
        use std::hash::{Hash, Hasher};

        let pair = |a: i64, b: i64| {
            Value::Tuple(TupleValue {
                elements: vec![Value::I64(a), Value::I64(b)],
            })
        };
        let k1 = DictKey::from_value(&pair(1, 2)).expect("composite key");
        let k2 = DictKey::from_value(&pair(1, 2)).expect("composite key");
        let k3 = DictKey::from_value(&pair(1, 3)).expect("composite key");

        // Equal tuples (separately constructed) → equal keys with equal hashes.
        assert!(matches!(k1, DictKey::Composite(_)));
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
        let hash = |k: &DictKey| {
            let mut s = DefaultHasher::new();
            k.hash(&mut s);
            s.finish()
        };
        assert_eq!(hash(&k1), hash(&k2));

        // Set dedup uses DictKey equality.
        let mut set = SetValue::new();
        assert!(set.insert(k1));
        assert!(!set.insert(k2)); // duplicate
        assert!(set.insert(k3));
        assert_eq!(set.len(), 2);

        // Round-trips back to the original value.
        let k4 = DictKey::from_value(&pair(5, 6)).expect("composite key");
        assert!(matches!(k4.to_value(), Value::Tuple(_)));
    }

    #[test]
    fn composite_key_rejects_unresolved_structref_6693() {
        use crate::vm::value::TupleValue;
        // A composite value carrying a heap `StructRef` must be rejected — callers
        // resolve heap refs to inline snapshots first, so a stray ref is a bug and
        // must not silently hash a heap index.
        let v = Value::Tuple(TupleValue {
            elements: vec![Value::StructRef(0)],
        });
        assert!(DictKey::from_value(&v).is_err());
    }

    #[test]
    fn test_dict_insert_and_get() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        d.insert(DictKey::Str("b".into()), Value::I64(2));
        d.insert(DictKey::I64(42), Value::Str("hello".into()));

        assert_eq!(d.len(), 3);
        assert!(!d.is_empty());
        assert!(matches!(
            d.get(&DictKey::Str("a".into())),
            Some(Value::I64(1))
        ));
        assert!(matches!(
            d.get(&DictKey::Str("b".into())),
            Some(Value::I64(2))
        ));
        assert!(matches!(d.get(&DictKey::I64(42)), Some(Value::Str(s)) if s == "hello"));
        assert!(d.get(&DictKey::Str("c".into())).is_none());
    }

    #[test]
    fn test_dict_update_existing_key() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        assert!(matches!(
            d.get(&DictKey::Str("a".into())),
            Some(Value::I64(1))
        ));

        d.insert(DictKey::Str("a".into()), Value::I64(100));
        assert!(matches!(
            d.get(&DictKey::Str("a".into())),
            Some(Value::I64(100))
        ));
        assert_eq!(d.len(), 1); // Length unchanged
    }

    #[test]
    fn test_dict_remove() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        d.insert(DictKey::Str("b".into()), Value::I64(2));
        d.insert(DictKey::Str("c".into()), Value::I64(3));

        let removed = d.remove(&DictKey::Str("b".into()));
        assert!(matches!(removed, Some(Value::I64(2))));
        assert_eq!(d.len(), 2);
        assert!(!d.contains_key(&DictKey::Str("b".into())));
        assert!(d.contains_key(&DictKey::Str("a".into())));
        assert!(d.contains_key(&DictKey::Str("c".into())));

        // Remove non-existent key
        let removed2 = d.remove(&DictKey::Str("x".into()));
        assert!(removed2.is_none());
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn test_dict_contains_key() {
        let mut d = DictValue::new();
        d.insert(DictKey::I64(1), Value::Str("one".into()));
        assert!(d.contains_key(&DictKey::I64(1)));
        assert!(!d.contains_key(&DictKey::I64(2)));
    }

    #[test]
    fn test_dict_narrow_integer_keys_issue_4633() {
        let mut d = DictValue::new();
        let key = DictKey::from_value(&Value::I8(3)).expect("Int8 should be a valid dict key");
        d.insert(key.clone(), Value::I16(4));

        assert_eq!(key.type_name(), "Int8");
        assert!(matches!(key.to_value(), Value::I8(3)));
        assert!(d.contains_key(&DictKey::I8(3)));
        assert!(d.contains_key(&DictKey::I64(3)));
        assert!(matches!(d.get(&DictKey::I16(3)), Some(Value::I16(4))));
    }

    #[test]
    fn test_dict_float_keys_issue_4638() {
        let mut d = DictValue::new();
        let key =
            DictKey::from_value(&Value::F32(1.0)).expect("Float32 should be a valid dict key");
        d.insert(key.clone(), Value::I16(4));

        assert_eq!(key.type_name(), "Float32");
        assert!(matches!(key.to_value(), Value::F32(1.0)));
        assert!(d.contains_key(&DictKey::F32(1.0f32.to_bits())));
        assert!(d.contains_key(&DictKey::F64(1.0f64.to_bits())));
        assert!(d.contains_key(&DictKey::I64(1)));
        assert!(matches!(
            d.get(&DictKey::F64(1.0f64.to_bits())),
            Some(Value::I16(4))
        ));
        assert!(!d.contains_key(&DictKey::F64((-0.0f64).to_bits())));
    }

    #[test]
    fn test_dict_type_object_keys_issue_5108() {
        use crate::types::JuliaType;

        // Type objects are valid Dict keys; equal types collapse to the same
        // key while distinct types do not (Issue #5108).
        let int_key = DictKey::from_value(&Value::DataType(Box::new(JuliaType::Int64)))
            .expect("DataType should be a valid dict key");
        let float_key = DictKey::from_value(&Value::DataType(Box::new(JuliaType::Float64)))
            .expect("DataType should be a valid dict key");

        assert_eq!(int_key.type_name(), "DataType");
        assert!(matches!(
            int_key.to_value(),
            Value::DataType(jt) if matches!(*jt, JuliaType::Int64)
        ));

        // Equal type => equal key (insert + lookup + overwrite)
        let mut d = DictValue::new();
        d.insert(int_key.clone(), Value::I64(1));
        d.insert(float_key.clone(), Value::I64(2));
        assert_eq!(d.len(), 2);
        assert!(d.contains_key(&int_key));
        assert!(d.contains_key(&float_key));
        let int_key_again =
            DictKey::from_value(&Value::DataType(Box::new(JuliaType::Int64))).unwrap();
        assert!(d.contains_key(&int_key_again));
        d.insert(int_key_again, Value::I64(10));
        assert_eq!(d.len(), 2);
        assert!(matches!(d.get(&int_key), Some(Value::I64(10))));

        // Distinct types are distinct keys
        assert_ne!(int_key, float_key);

        // Alternate JuliaType projections of the same semantic type collapse:
        // `VectorOf(Int64)` and `Struct("Vector{Int64}")` must hash/compare equal.
        let vec_a = DictKey::from_value(&Value::DataType(Box::new(JuliaType::VectorOf(Box::new(
            JuliaType::Int64,
        )))))
        .unwrap();
        let vec_b = DictKey::from_value(&Value::DataType(Box::new(JuliaType::Struct(
            "Vector{Int64}".into(),
        ))))
        .unwrap();
        assert_eq!(vec_a, vec_b);

        // A type key is distinct from a same-shaped non-type key.
        assert_ne!(int_key, DictKey::I64(1));
    }

    #[test]
    fn test_dict_with_entries() {
        let entries = vec![
            (DictKey::Str("x".into()), Value::I64(10)),
            (DictKey::Str("y".into()), Value::I64(20)),
        ];
        let d = DictValue::with_entries(entries);
        assert_eq!(d.len(), 2);
        assert!(matches!(
            d.get(&DictKey::Str("x".into())),
            Some(Value::I64(10))
        ));
        assert!(matches!(
            d.get(&DictKey::Str("y".into())),
            Some(Value::I64(20))
        ));
    }

    #[test]
    fn test_dict_with_type_params() {
        let d = DictValue::with_type_params("String".into(), "Int64".into());
        assert_eq!(d.key_type.as_deref(), Some("String"));
        assert_eq!(d.value_type.as_deref(), Some("Int64"));
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn test_dict_keys_values() {
        let mut d = DictValue::new();
        d.insert(DictKey::I64(1), Value::Str("a".into()));
        d.insert(DictKey::I64(2), Value::Str("b".into()));

        let keys = d.keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&DictKey::I64(1)));
        assert!(keys.contains(&DictKey::I64(2)));

        let vals = d.values();
        assert_eq!(vals.len(), 2);
    }

    #[test]
    fn test_dict_iter() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        d.insert(DictKey::Str("b".into()), Value::I64(2));

        let pairs: Vec<_> = d.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn test_dict_merge() {
        let mut d1 = DictValue::new();
        d1.insert(DictKey::Str("a".into()), Value::I64(1));
        d1.insert(DictKey::Str("b".into()), Value::I64(2));

        let mut d2 = DictValue::new();
        d2.insert(DictKey::Str("b".into()), Value::I64(20));
        d2.insert(DictKey::Str("c".into()), Value::I64(30));

        d1.merge(&d2);
        assert_eq!(d1.len(), 3);
        assert!(matches!(
            d1.get(&DictKey::Str("a".into())),
            Some(Value::I64(1))
        ));
        assert!(matches!(
            d1.get(&DictKey::Str("b".into())),
            Some(Value::I64(20))
        ));
        assert!(matches!(
            d1.get(&DictKey::Str("c".into())),
            Some(Value::I64(30))
        ));
    }

    #[test]
    fn test_dict_clear() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        d.insert(DictKey::Str("b".into()), Value::I64(2));
        assert_eq!(d.len(), 2);

        d.clear();
        assert_eq!(d.len(), 0);
        assert!(d.is_empty());
        assert!(d.get(&DictKey::Str("a".into())).is_none());

        // Can insert after clear
        d.insert(DictKey::Str("c".into()), Value::I64(3));
        assert_eq!(d.len(), 1);
        assert!(matches!(
            d.get(&DictKey::Str("c".into())),
            Some(Value::I64(3))
        ));
    }

    #[test]
    fn test_dict_rehash_with_many_entries() {
        let mut d = DictValue::new();
        // Insert enough entries to trigger rehash (initial table size is 16)
        for i in 0..50 {
            d.insert(DictKey::I64(i), Value::I64(i * 10));
        }
        assert_eq!(d.len(), 50);

        // Verify all entries are still accessible
        for i in 0..50 {
            assert!(
                matches!(d.get(&DictKey::I64(i)), Some(Value::I64(v)) if *v == i * 10),
                "Failed to get key {} after rehash",
                i
            );
        }
    }

    #[test]
    fn test_dict_delete_and_reinsert() {
        let mut d = DictValue::new();
        d.insert(DictKey::I64(1), Value::I64(10));
        d.insert(DictKey::I64(2), Value::I64(20));
        d.insert(DictKey::I64(3), Value::I64(30));

        d.remove(&DictKey::I64(2));
        assert!(!d.contains_key(&DictKey::I64(2)));
        assert_eq!(d.len(), 2);

        // Reinsert at the same key
        d.insert(DictKey::I64(2), Value::I64(200));
        assert!(matches!(d.get(&DictKey::I64(2)), Some(Value::I64(200))));
        assert_eq!(d.len(), 3);
    }

    #[test]
    fn test_dict_next_filled_slot() {
        let mut d = DictValue::new();
        d.insert(DictKey::I64(1), Value::I64(10));
        d.insert(DictKey::I64(2), Value::I64(20));

        // First filled slot from 0
        let first = d.next_filled_slot(0);
        assert!(first.is_some());

        // Scan all entries
        let mut count = 0;
        let mut idx = 0;
        while let Some((slot_idx, _, _)) = d.next_filled_slot(idx) {
            count += 1;
            idx = slot_idx + 1;
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn test_table_size_function() {
        assert_eq!(table_size(0), 0);
        assert_eq!(table_size(1), MIN_DICT_TABLE_SIZE); // minimum table size
        assert_eq!(table_size(4), MIN_DICT_TABLE_SIZE);
        assert_eq!(table_size(MIN_DICT_TABLE_SIZE), MIN_DICT_TABLE_SIZE);
        assert_eq!(table_size(17), 32);
        assert_eq!(table_size(32), 32);
        assert_eq!(table_size(33), 64);
    }

    #[test]
    fn test_shorthash7_has_high_bit_set() {
        // shorthash7 should always have the high bit (0x80) set
        for i in 0..100u64 {
            let sh = shorthash7(i * 1234567890);
            assert!((sh & 0x80) != 0, "shorthash7 must have high bit set");
        }
    }

    #[test]
    fn test_slot_metadata_constants() {
        assert!(is_slot_empty(SLOT_EMPTY));
        assert!(!is_slot_filled(SLOT_EMPTY));
        assert!(!is_slot_deleted(SLOT_EMPTY));

        assert!(is_slot_deleted(SLOT_DELETED));
        assert!(!is_slot_filled(SLOT_DELETED));
        assert!(!is_slot_empty(SLOT_DELETED));

        // A filled slot has high bit set
        let filled = 0x83u8; // 0x80 | 0x03
        assert!(is_slot_filled(filled));
        assert!(!is_slot_empty(filled));
        assert!(!is_slot_deleted(filled));
    }

    #[test]
    fn test_dict_symbol_keys() {
        let mut d = DictValue::new();
        d.insert(DictKey::Symbol("x".into()), Value::I64(1));
        d.insert(DictKey::Symbol("y".into()), Value::I64(2));

        assert!(d.contains_key(&DictKey::Symbol("x".into())));
        assert!(matches!(
            d.get(&DictKey::Symbol("y".into())),
            Some(Value::I64(2))
        ));
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn test_dict_mixed_key_types() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("hello".into()), Value::I64(1));
        d.insert(DictKey::I64(42), Value::I64(2));
        d.insert(DictKey::Symbol("sym".into()), Value::I64(3));

        assert_eq!(d.len(), 3);
        assert!(matches!(
            d.get(&DictKey::Str("hello".into())),
            Some(Value::I64(1))
        ));
        assert!(matches!(d.get(&DictKey::I64(42)), Some(Value::I64(2))));
        assert!(matches!(
            d.get(&DictKey::Symbol("sym".into())),
            Some(Value::I64(3))
        ));
    }

    #[test]
    fn test_dict_clone() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        d.insert(DictKey::Str("b".into()), Value::I64(2));

        let d2 = d.clone();
        assert_eq!(d2.len(), 2);
        assert!(matches!(
            d2.get(&DictKey::Str("a".into())),
            Some(Value::I64(1))
        ));

        // Modifying original doesn't affect clone
        d.insert(DictKey::Str("c".into()), Value::I64(3));
        assert_eq!(d.len(), 3);
        assert_eq!(d2.len(), 2);
    }

    // Issue #5187: the borrowed-key probe (`KeyRef`) must hash byte-identically
    // to the owned `DictKey` so that `get_by_value` / `contains_key_by_value`
    // land in exactly the same probe bucket as the owned read path. These tests
    // are the TDD safety net for the no-clone string/symbol read path.

    /// Hashing parity: a borrowed `KeyRef` must produce the exact same
    /// `DefaultHasher` output (and therefore `hashindex`/`shorthash7`) as the
    /// owned `DictKey` it mirrors, for every key flavor.
    #[test]
    fn test_keyref_hash_parity_with_dictkey_issue_5187() {
        let cases: Vec<(DictKey, Value)> = vec![
            (DictKey::Str("hello".into()), Value::Str("hello".into())),
            (DictKey::Str(String::new()), Value::Str(String::new())),
            (
                DictKey::Symbol("sym".into()),
                Value::Symbol(super::super::macro_::SymbolValue::new("sym")),
            ),
            (DictKey::I64(42), Value::I64(42)),
            (DictKey::I8(3), Value::I8(3)),
            (DictKey::U64(7), Value::U64(7)),
            (DictKey::F64(1.5f64.to_bits()), Value::F64(1.5)),
            (DictKey::F32(2.0f32.to_bits()), Value::F32(2.0)),
        ];
        for (owned, value) in cases {
            let key_ref = KeyRef::from_value(&value)
                .unwrap_or_else(|| panic!("KeyRef::from_value failed for {value:?}"));
            assert_eq!(
                hash_dict_key(&owned),
                hash_key_ref(&key_ref),
                "hash mismatch for {owned:?}"
            );
            assert!(
                key_ref.matches_dict_key(&owned),
                "equality mismatch for {owned:?}"
            );
        }
    }

    /// Read parity: `get_by_value` / `contains_key_by_value` must agree with
    /// the owned `get` / `contains_key` for hits and misses across string,
    /// symbol, and numeric keys, including after a rehash.
    #[test]
    fn test_dict_get_by_value_parity_issue_5187() {
        // `Value` has no `PartialEq`; the stored values here are all `I64`, so
        // compare via the payload.
        fn as_i64(v: Option<&Value>) -> Option<i64> {
            match v {
                Some(Value::I64(n)) => Some(*n),
                _ => None,
            }
        }

        let mut d = DictValue::new();
        // Mix many string keys to force a rehash, plus symbol + numeric keys.
        for i in 0..50 {
            d.insert(DictKey::Str(format!("k{i}")), Value::I64(i));
        }
        d.insert(DictKey::Symbol("alpha".into()), Value::I64(1000));
        d.insert(DictKey::I64(7), Value::I64(777));

        for i in 0..50 {
            let probe = Value::Str(format!("k{i}"));
            let owned = DictKey::Str(format!("k{i}"));
            assert_eq!(
                as_i64(d.get_by_value(&probe)),
                as_i64(d.get(&owned)),
                "string hit parity failed for k{i}"
            );
            assert_eq!(as_i64(d.get_by_value(&probe)), Some(i));
            assert!(d.contains_key_by_value(&probe));
        }

        // Symbol key parity
        let sym_probe = Value::Symbol(super::super::macro_::SymbolValue::new("alpha"));
        assert_eq!(
            as_i64(d.get_by_value(&sym_probe)),
            as_i64(d.get(&DictKey::Symbol("alpha".into())))
        );
        assert_eq!(as_i64(d.get_by_value(&sym_probe)), Some(1000));
        assert!(d.contains_key_by_value(&sym_probe));

        // Numeric key parity
        assert_eq!(
            as_i64(d.get_by_value(&Value::I64(7))),
            as_i64(d.get(&DictKey::I64(7)))
        );
        assert_eq!(as_i64(d.get_by_value(&Value::I64(7))), Some(777));

        // Misses
        assert!(d.get_by_value(&Value::Str("missing".into())).is_none());
        assert!(!d.contains_key_by_value(&Value::Str("missing".into())));
        assert!(d
            .get_by_value(&Value::Symbol(super::super::macro_::SymbolValue::new(
                "nope"
            )))
            .is_none());
        assert!(d.get_by_value(&Value::I64(999)).is_none());

        // A String probe must never collide with a Symbol key of the same text
        // (and vice versa): they carry different discriminants.
        let mut d2 = DictValue::new();
        d2.insert(DictKey::Symbol("x".into()), Value::I64(1));
        assert!(d2.get_by_value(&Value::Str("x".into())).is_none());
        let mut d3 = DictValue::new();
        d3.insert(DictKey::Str("x".into()), Value::I64(1));
        assert!(d3
            .get_by_value(&Value::Symbol(super::super::macro_::SymbolValue::new("x")))
            .is_none());
    }

    /// An invalid borrowed key (one that has no `DictKey` representation) must
    /// report a miss rather than panic, matching the owned `from_value` Err path.
    #[test]
    fn test_dict_get_by_value_invalid_key_issue_5187() {
        let mut d = DictValue::new();
        d.insert(DictKey::Str("a".into()), Value::I64(1));
        assert!(d.get_by_value(&Value::Nothing).is_none());
        assert!(!d.contains_key_by_value(&Value::Nothing));
    }
    // ---- Internal FxHash dict-key hashing (Issue #5188) ----

    /// The hand-rolled FxHasher must be deterministic: hashing the same bytes
    /// twice yields the same digest within a process.
    #[test]
    fn test_fxhasher_is_deterministic_5188() {
        let mut a = FxHasher::default();
        let mut b = FxHasher::default();
        0x1234_5678_9abc_def0u64.hash(&mut a);
        "hello".hash(&mut a);
        0x1234_5678_9abc_def0u64.hash(&mut b);
        "hello".hash(&mut b);
        assert_eq!(a.finish(), b.finish());
    }

    /// `hash_dict_key` must be consistent with `PartialEq`: keys that compare
    /// equal (even across numeric representations) must hash identically, so
    /// they land in the same open-addressing slot.
    #[test]
    fn test_hash_dict_key_consistent_with_eq_5188() {
        // Cross-type numeric equality: 3i8 == 3i64 == 3u32 == 3.0
        let keys = [
            DictKey::I8(3),
            DictKey::I64(3),
            DictKey::U32(3),
            DictKey::F64(3.0f64.to_bits()),
        ];
        for k in &keys {
            assert_eq!(*k, keys[0]);
            assert_eq!(
                hash_dict_key(k),
                hash_dict_key(&keys[0]),
                "equal keys must share a dict-slot hash"
            );
        }
        // +0.0 and -0.0 are NOT isequal in Julia => distinct keys; hashing must
        // not be required to collide, but each must be self-consistent.
        let pos_zero = DictKey::F64(0.0f64.to_bits());
        let neg_zero = DictKey::F64((-0.0f64).to_bits());
        assert_ne!(pos_zero, neg_zero);
        assert_eq!(hash_dict_key(&pos_zero), hash_dict_key(&pos_zero));
        assert_eq!(hash_dict_key(&neg_zero), hash_dict_key(&neg_zero));
    }

    /// A collision-prone, resize-heavy workload with mixed key kinds must round
    /// trip every entry. This exercises the internal FxHash path under load.
    #[test]
    fn test_dict_stress_mixed_keys_resize_5188() {
        let mut d = DictValue::new();
        // Integer keys (sequential => stresses the low bits of the hash).
        for i in 0..500i64 {
            d.insert(DictKey::I64(i), Value::I64(i * 2));
        }
        // String keys with a shared prefix (collision-prone tails).
        for i in 0..500i64 {
            d.insert(DictKey::Str(format!("key_{i}")), Value::I64(i));
        }
        assert_eq!(d.len(), 1000);
        for i in 0..500i64 {
            assert!(
                matches!(d.get(&DictKey::I64(i)), Some(Value::I64(v)) if *v == i * 2),
                "int key {i} lost after resize",
            );
            assert!(
                matches!(d.get(&DictKey::Str(format!("key_{i}"))), Some(Value::I64(v)) if *v == i),
                "string key {i} lost after resize",
            );
        }
        // Delete every other integer key, then reinsert with new values.
        for i in (0..500i64).step_by(2) {
            assert!(d.remove(&DictKey::I64(i)).is_some());
        }
        for i in (0..500i64).step_by(2) {
            d.insert(DictKey::I64(i), Value::I64(i * 3));
        }
        for i in 0..500i64 {
            let expected = if i % 2 == 0 { i * 3 } else { i * 2 };
            assert!(
                matches!(d.get(&DictKey::I64(i)), Some(Value::I64(v)) if *v == expected),
                "int key {i} wrong after delete/reinsert",
            );
        }
    }
}
