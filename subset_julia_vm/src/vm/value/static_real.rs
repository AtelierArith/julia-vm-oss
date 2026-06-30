//! Flat representation for small homogeneous-Real SVector{N,T} / SMatrix{M,N,T}
//! (Issue #7964, Phase 1 + Phase 2).
//!
//! SVector/SMatrix with any <:Real element type and shape ≤ 4×4 are stored as a
//! flat typed buffer instead of a heap `Vec<Value>` tuple.  This eliminates:
//! - One heap `Vec<Value>` per value (tuple boxing)
//! - struct_heap growth in hot loops (no StructRef slot per iteration)
//! - Per-element `Value::*` tag union overhead on every `.data[i]` read
//!
//! Julia-level transparency: `typeof`, `isa`, `size`, `length`, `eltype`,
//! field access (`.data`), and tuple indexing (`d[i]`) all behave as if the
//! value were the original `Value::Struct + Value::Tuple` pair.
//!
//! Fallback: `to_tuple_value()` materialises the standard `TupleValue` for
//! operations not yet specialised on this representation.
//!
//! Storage convention: **column-major** (Julia/SMatrix convention).
//! Element (i,j) of an M×N SMatrix lives at `elems[i + j*M]` (0-indexed).
//! SVector{N} is treated as an N×1 matrix.
//!
//! Phase 2 note: arithmetic results carry the type name as a pre-allocated
//! `Box<str>` cloned from the source operand when possible, so common
//! type strings (e.g. `"SVector{2, Float64}"`) are NOT re-formatted on
//! every arithmetic operation in the normal case.

#![allow(clippy::cast_sign_loss)]

use crate::vm::error::VmError;
use crate::vm::value::{TupleValue, Value};

/// Typed flat storage for a homogeneous-Real static array.
#[derive(Debug, Clone)]
pub enum StaticElem {
    F64(Vec<f64>),
    F32(Vec<f32>),
    I64(Vec<i64>),
    I32(Vec<i32>),
    I16(Vec<i16>),
    I8(Vec<i8>),
    U64(Vec<u64>),
    U32(Vec<u32>),
    U16(Vec<u16>),
    U8(Vec<u8>),
    Bool(Vec<bool>),
}

impl StaticElem {
    pub fn len(&self) -> usize {
        match self {
            Self::F64(v) => v.len(),
            Self::F32(v) => v.len(),
            Self::I64(v) => v.len(),
            Self::I32(v) => v.len(),
            Self::I16(v) => v.len(),
            Self::I8(v) => v.len(),
            Self::U64(v) => v.len(),
            Self::U32(v) => v.len(),
            Self::U16(v) => v.len(),
            Self::U8(v) => v.len(),
            Self::Bool(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Julia element-type name for this buffer (e.g. `"Int64"`), used so that
    /// `eltype` reports the concrete element type of a heap `StaticArray`
    /// carrier instead of widening to `Any` (Issue #8131).
    pub fn element_type_name(&self) -> &'static str {
        match self {
            Self::F64(_) => "Float64",
            Self::F32(_) => "Float32",
            Self::I64(_) => "Int64",
            Self::I32(_) => "Int32",
            Self::I16(_) => "Int16",
            Self::I8(_) => "Int8",
            Self::U64(_) => "UInt64",
            Self::U32(_) => "UInt32",
            Self::U16(_) => "UInt16",
            Self::U8(_) => "UInt8",
            Self::Bool(_) => "Bool",
        }
    }

    /// Read element at 0-based offset as a `Value`.
    pub fn get_value(&self, i: usize) -> Option<Value> {
        Some(match self {
            Self::F64(v) => Value::F64(*v.get(i)?),
            Self::F32(v) => Value::F32(*v.get(i)?),
            Self::I64(v) => Value::I64(*v.get(i)?),
            Self::I32(v) => Value::I32(*v.get(i)?),
            Self::I16(v) => Value::I16(*v.get(i)?),
            Self::I8(v) => Value::I8(*v.get(i)?),
            Self::U64(v) => Value::U64(*v.get(i)?),
            Self::U32(v) => Value::U32(*v.get(i)?),
            Self::U16(v) => Value::U16(*v.get(i)?),
            Self::U8(v) => Value::U8(*v.get(i)?),
            Self::Bool(v) => Value::Bool(*v.get(i)?),
        })
    }

    /// Materialise all elements as a `Vec<Value>` (fallback path).
    pub fn to_values(&self) -> Vec<Value> {
        (0..self.len()).filter_map(|i| self.get_value(i)).collect()
    }
}

/// Flat, unboxed representation for a small homogeneous-Real static array.
///
/// Covers `SVector{N, T}` (cols == 1) and `SMatrix{M, N, T}` (rows == M,
/// cols == N) for any `T <: Real` with shape ≤ 4.  Elements are stored
/// column-major (Julia convention): element (i, j) lives at
/// `elems[i + j * rows]` (0-indexed).
#[derive(Debug, Clone)]
pub struct StaticRealValue {
    /// Fully-qualified Julia type name, e.g. `"SVector{2, Float64}"`.
    pub type_name: Box<str>,
    /// Number of rows (N for SVector, M for SMatrix).
    pub rows: usize,
    /// Number of columns (1 for SVector, N for SMatrix).
    pub cols: usize,
    /// Typed flat element buffer.
    pub elems: StaticElem,
}

impl StaticRealValue {
    pub fn new_vector(type_name: impl Into<Box<str>>, elems: StaticElem) -> Self {
        let n = elems.len();
        Self {
            type_name: type_name.into(),
            rows: n,
            cols: 1,
            elems,
        }
    }

    pub fn new_matrix(
        type_name: impl Into<Box<str>>,
        rows: usize,
        cols: usize,
        elems: StaticElem,
    ) -> Self {
        debug_assert_eq!(elems.len(), rows * cols);
        Self {
            type_name: type_name.into(),
            rows,
            cols,
            elems,
        }
    }

    pub fn is_vector(&self) -> bool {
        self.cols == 1
    }

    /// Total number of elements.
    pub fn len(&self) -> usize {
        self.elems.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elems.is_empty()
    }

    /// Get element at 1-based **linear** index (Julia convention).
    pub fn get_1d(&self, index: i64) -> Result<Value, VmError> {
        let n = self.elems.len();
        if index < 1 || index as usize > n {
            return Err(VmError::TupleIndexOutOfBounds { index, length: n });
        }
        self.elems
            .get_value((index - 1) as usize)
            .ok_or(VmError::TupleIndexOutOfBounds { index, length: n })
    }

    /// Materialise as a `TupleValue` for fallback operations.
    pub fn to_tuple_value(&self) -> TupleValue {
        TupleValue::new(self.elems.to_values())
    }

    /// Julia type name string (same as `type_name`).
    pub fn julia_type_name(&self) -> &str {
        &self.type_name
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Phase 3: Zero-allocation inline storage (Issue #7964)
// ──────────────────────────────────────────────────────────────────────────────

/// Element-type tag for `StaticArrayInlineData`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InlineElemTag {
    F64 = 0,
    F32 = 1,
    I64 = 2,
    I32 = 3,
    I16 = 4,
    I8 = 5,
    U64 = 6,
    U32 = 7,
    U16 = 8,
    U8 = 9,
    Bool = 10,
}

impl InlineElemTag {
    pub fn julia_name(self) -> &'static str {
        match self {
            Self::F64 => "Float64",
            Self::F32 => "Float32",
            Self::I64 => "Int64",
            Self::I32 => "Int32",
            Self::I16 => "Int16",
            Self::I8 => "Int8",
            Self::U64 => "UInt64",
            Self::U32 => "UInt32",
            Self::U16 => "UInt16",
            Self::U8 => "UInt8",
            Self::Bool => "Bool",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "Float64" => Some(Self::F64),
            "Float32" => Some(Self::F32),
            "Int64" => Some(Self::I64),
            "Int32" => Some(Self::I32),
            "Int16" => Some(Self::I16),
            "Int8" => Some(Self::I8),
            "UInt64" => Some(Self::U64),
            "UInt32" => Some(Self::U32),
            "UInt16" => Some(Self::U16),
            "UInt8" => Some(Self::U8),
            "Bool" => Some(Self::Bool),
            _ => None,
        }
    }
}

/// Zero-allocation inline storage for small SVector/SMatrix with N ≤ 4
/// elements of any `<:Real` type (Issue #7964 Phase 3).
///
/// Fits in 40 bytes payload — well within the 48-byte max for the `Value`
/// enum (alignment 16 due to I128/U128).  `Copy` means the value lives
/// entirely on the VM stack without any heap allocation.
///
/// Elements are column-major (Julia convention): element (i,j) of an M×N
/// matrix is at `data[i + j*rows]` (0-indexed, reinterpreted from `u64` bits
/// according to `tag`).
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct StaticArrayInlineData {
    pub rows: u8,
    pub cols: u8,
    pub n: u8, // rows * cols
    pub tag: InlineElemTag,
    _pad: [u8; 4],
    pub data: [u64; 4],
}

#[allow(clippy::needless_range_loop)]
impl StaticArrayInlineData {
    pub fn rows(&self) -> usize {
        self.rows as usize
    }
    pub fn cols(&self) -> usize {
        self.cols as usize
    }
    pub fn is_vector(&self) -> bool {
        self.cols == 1
    }
    pub fn len(&self) -> usize {
        self.n as usize
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// Julia type name as a `&'static str` — no allocation.
    pub fn julia_type_name_static(&self) -> &'static str {
        let et = self.tag.julia_name();
        let (rows, cols) = (self.rows, self.cols);
        match (rows, cols, et) {
            (1, 1, "Float64") => "SVector{1, Float64}",
            (2, 1, "Float64") => "SVector{2, Float64}",
            (3, 1, "Float64") => "SVector{3, Float64}",
            (4, 1, "Float64") => "SVector{4, Float64}",
            (1, 1, "Float32") => "SVector{1, Float32}",
            (2, 1, "Float32") => "SVector{2, Float32}",
            (3, 1, "Float32") => "SVector{3, Float32}",
            (4, 1, "Float32") => "SVector{4, Float32}",
            (1, 1, "Int64") => "SVector{1, Int64}",
            (2, 1, "Int64") => "SVector{2, Int64}",
            (3, 1, "Int64") => "SVector{3, Int64}",
            (4, 1, "Int64") => "SVector{4, Int64}",
            (1, 1, "Int32") => "SVector{1, Int32}",
            (2, 1, "Int32") => "SVector{2, Int32}",
            (3, 1, "Int32") => "SVector{3, Int32}",
            (4, 1, "Int32") => "SVector{4, Int32}",
            (2, 2, "Float64") => "SMatrix{2, 2, Float64}",
            (3, 3, "Float64") => "SMatrix{3, 3, Float64}",
            (4, 4, "Float64") => "SMatrix{4, 4, Float64}",
            (2, 3, "Float64") => "SMatrix{2, 3, Float64}",
            (3, 2, "Float64") => "SMatrix{3, 2, Float64}",
            (2, 2, "Float32") => "SMatrix{2, 2, Float32}",
            (3, 3, "Float32") => "SMatrix{3, 3, Float32}",
            (2, 2, "Int64") => "SMatrix{2, 2, Int64}",
            (3, 3, "Int64") => "SMatrix{3, 3, Int64}",
            _ => "", // uncommon — caller falls back to format!
        }
    }

    /// Julia type name as an owned `Box<str>` (allocates only for rare sizes).
    pub fn julia_type_name_owned(&self) -> Box<str> {
        let s = self.julia_type_name_static();
        if !s.is_empty() {
            Box::from(s)
        } else {
            let et = self.tag.julia_name();
            if self.is_vector() {
                format!("SVector{{{}, {}}}", self.rows, et).into()
            } else {
                format!("SMatrix{{{}, {}, {}}}", self.rows, self.cols, et).into()
            }
        }
    }

    /// Read element at 1-based linear index (Julia convention).
    pub fn get_1d(&self, index: i64) -> Result<Value, VmError> {
        let n = self.n as usize;
        if index < 1 || index as usize > n {
            return Err(VmError::TupleIndexOutOfBounds { index, length: n });
        }
        Ok(self.get_0indexed((index - 1) as usize))
    }

    /// Read element at 0-based index.
    pub fn get_0indexed(&self, i: usize) -> Value {
        let raw = self.data[i];
        match self.tag {
            InlineElemTag::F64 => Value::F64(f64::from_bits(raw)),
            InlineElemTag::F32 => Value::F32(f32::from_bits(raw as u32)),
            InlineElemTag::I64 => Value::I64(raw as i64),
            InlineElemTag::I32 => Value::I32(raw as i32),
            InlineElemTag::I16 => Value::I16(raw as i16),
            InlineElemTag::I8 => Value::I8(raw as i8),
            InlineElemTag::U64 => Value::U64(raw),
            InlineElemTag::U32 => Value::U32(raw as u32),
            InlineElemTag::U16 => Value::U16(raw as u16),
            InlineElemTag::U8 => Value::U8(raw as u8),
            InlineElemTag::Bool => Value::Bool(raw != 0),
        }
    }

    /// Materialise as a `TupleValue` (fallback for operations not yet specialised).
    pub fn to_tuple_value(&self) -> TupleValue {
        TupleValue::new((0..self.n as usize).map(|i| self.get_0indexed(i)).collect())
    }

    // ── Constructors ──────────────────────────────────────────────────────────

    /// Build from a `StaticElem` slice (used during struct construction).
    /// Returns `None` if the element count exceeds 4.
    pub fn try_from_elem(rows: usize, cols: usize, elems: &StaticElem) -> Option<Self> {
        let n = elems.len();
        if n > 4 {
            return None;
        }
        let mut data = [0u64; 4];
        let tag = match elems {
            StaticElem::F64(v) => {
                for (i, &x) in v.iter().enumerate() {
                    data[i] = x.to_bits();
                }
                InlineElemTag::F64
            }
            StaticElem::F32(v) => {
                for (i, &x) in v.iter().enumerate() {
                    data[i] = x.to_bits() as u64;
                }
                InlineElemTag::F32
            }
            StaticElem::I64(v) => {
                for (i, &x) in v.iter().enumerate() {
                    data[i] = x as u64;
                }
                InlineElemTag::I64
            }
            StaticElem::I32(v) => {
                for (i, &x) in v.iter().enumerate() {
                    data[i] = x as u64;
                }
                InlineElemTag::I32
            }
            StaticElem::I16(v) => {
                for (i, &x) in v.iter().enumerate() {
                    data[i] = x as u64;
                }
                InlineElemTag::I16
            }
            StaticElem::I8(v) => {
                for (i, &x) in v.iter().enumerate() {
                    data[i] = x as u64;
                }
                InlineElemTag::I8
            }
            StaticElem::U64(v) => {
                for (i, &x) in v.iter().enumerate() {
                    data[i] = x;
                }
                InlineElemTag::U64
            }
            StaticElem::U32(v) => {
                for (i, &x) in v.iter().enumerate() {
                    data[i] = x as u64;
                }
                InlineElemTag::U32
            }
            StaticElem::U16(v) => {
                for (i, &x) in v.iter().enumerate() {
                    data[i] = x as u64;
                }
                InlineElemTag::U16
            }
            StaticElem::U8(v) => {
                for (i, &x) in v.iter().enumerate() {
                    data[i] = x as u64;
                }
                InlineElemTag::U8
            }
            StaticElem::Bool(v) => {
                for (i, &x) in v.iter().enumerate() {
                    data[i] = x as u64;
                }
                InlineElemTag::Bool
            }
        };
        Some(Self {
            rows: rows as u8,
            cols: cols as u8,
            n: n as u8,
            tag,
            _pad: [0; 4],
            data,
        })
    }

    // ── Phase 3 arithmetic (pure stack, zero allocation) ─────────────────────

    fn new_vector_raw(rows: usize, tag: InlineElemTag, data: [u64; 4]) -> Self {
        Self {
            rows: rows as u8,
            cols: 1,
            n: rows as u8,
            tag,
            _pad: [0; 4],
            data,
        }
    }
    fn new_matrix_raw(rows: usize, cols: usize, tag: InlineElemTag, data: [u64; 4]) -> Self {
        Self {
            rows: rows as u8,
            cols: cols as u8,
            n: (rows * cols) as u8,
            tag,
            _pad: [0; 4],
            data,
        }
    }

    pub fn inline_add(&self, other: &Self) -> Option<Value> {
        if self.rows != other.rows || self.cols != other.cols || self.tag != other.tag {
            return None;
        }
        let (tag, n) = (self.tag, self.n as usize);
        let mut data = [0u64; 4];
        for i in 0..n {
            data[i] = match tag {
                InlineElemTag::F64 => {
                    (f64::from_bits(self.data[i]) + f64::from_bits(other.data[i])).to_bits()
                }
                InlineElemTag::F32 => {
                    let r =
                        f32::from_bits(self.data[i] as u32) + f32::from_bits(other.data[i] as u32);
                    r.to_bits() as u64
                }
                _ => (self.data[i]).wrapping_add(other.data[i]),
            };
        }
        let result = if self.is_vector() {
            Self::new_vector_raw(self.rows as usize, self.tag, data)
        } else {
            Self::new_matrix_raw(self.rows as usize, self.cols as usize, self.tag, data)
        };
        Some(Value::StaticArrayInline(result))
    }

    pub fn inline_sub(&self, other: &Self) -> Option<Value> {
        if self.rows != other.rows || self.cols != other.cols || self.tag != other.tag {
            return None;
        }
        let (tag, n) = (self.tag, self.n as usize);
        let mut data = [0u64; 4];
        for i in 0..n {
            data[i] = match tag {
                InlineElemTag::F64 => {
                    (f64::from_bits(self.data[i]) - f64::from_bits(other.data[i])).to_bits()
                }
                InlineElemTag::F32 => {
                    let r =
                        f32::from_bits(self.data[i] as u32) - f32::from_bits(other.data[i] as u32);
                    r.to_bits() as u64
                }
                _ => (self.data[i]).wrapping_sub(other.data[i]),
            };
        }
        let result = if self.is_vector() {
            Self::new_vector_raw(self.rows as usize, self.tag, data)
        } else {
            Self::new_matrix_raw(self.rows as usize, self.cols as usize, self.tag, data)
        };
        Some(Value::StaticArrayInline(result))
    }

    /// `SMatrix * SVector` → `SVector` (inline, zero allocation).
    ///
    /// Data layout: column-major (upstream StaticArrays / Julia convention,
    /// Issue #8084). A[i,j] of an m×k matrix is at data[j*m + i].
    pub fn inline_matvec(&self, vec: &Self) -> Option<Value> {
        if self.cols != vec.rows || !vec.is_vector() || self.tag != vec.tag {
            return None;
        }
        let (m, k) = (self.rows as usize, self.cols as usize);
        let tag = self.tag;
        let mut data = [0u64; 4];
        for i in 0..m {
            for j in 0..k {
                let a = self.data[j * m + i];
                let x = vec.data[j];
                data[i] = match tag {
                    InlineElemTag::F64 => {
                        (f64::from_bits(data[i]) + f64::from_bits(a) * f64::from_bits(x)).to_bits()
                    }
                    InlineElemTag::F32 => {
                        let r = f32::from_bits(data[i] as u32)
                            + f32::from_bits(a as u32) * f32::from_bits(x as u32);
                        r.to_bits() as u64
                    }
                    _ => data[i].wrapping_add(a.wrapping_mul(x)),
                };
            }
        }
        Some(Value::StaticArrayInline(Self::new_vector_raw(m, tag, data)))
    }

    /// `SMatrix * SMatrix` → `SMatrix` (inline, zero allocation).
    ///
    /// Column-major layout (upstream StaticArrays / Julia, Issue #8084):
    /// A[i,l] = self.data[l*ar + i], B[l,j] = other.data[j*k + l],
    /// C[i,j] = data[j*ar + i].
    pub fn inline_matmat(&self, other: &Self) -> Option<Value> {
        if self.cols != other.rows || self.is_vector() || other.is_vector() || self.tag != other.tag
        {
            return None;
        }
        let (ar, k, bc) = (self.rows as usize, self.cols as usize, other.cols as usize);
        if ar * bc > 4 {
            return None;
        }
        let tag = self.tag;
        let mut data = [0u64; 4];
        for i in 0..ar {
            for j in 0..bc {
                let c_idx = j * ar + i;
                for l in 0..k {
                    let a = self.data[l * ar + i];
                    let b = other.data[j * k + l];
                    data[c_idx] = match tag {
                        InlineElemTag::F64 => (f64::from_bits(data[c_idx])
                            + f64::from_bits(a) * f64::from_bits(b))
                        .to_bits(),
                        InlineElemTag::F32 => {
                            let r = f32::from_bits(data[c_idx] as u32)
                                + f32::from_bits(a as u32) * f32::from_bits(b as u32);
                            r.to_bits() as u64
                        }
                        _ => data[c_idx].wrapping_add(a.wrapping_mul(b)),
                    };
                }
            }
        }
        Some(Value::StaticArrayInline(Self::new_matrix_raw(
            ar, bc, tag, data,
        )))
    }

    /// `scalar * SVector/SMatrix` → same shape inline (zero allocation for matching types).
    pub fn inline_scalar_mul(&self, scalar: &Value) -> Option<Value> {
        let n = self.n as usize;
        let mut data = [0u64; 4];
        let tag = match (scalar, self.tag) {
            (Value::F64(s), InlineElemTag::F64) => {
                for i in 0..n {
                    data[i] = (f64::from_bits(self.data[i]) * s).to_bits();
                }
                InlineElemTag::F64
            }
            (Value::F32(s), InlineElemTag::F32) => {
                for i in 0..n {
                    data[i] = (f32::from_bits(self.data[i] as u32) * s).to_bits() as u64;
                }
                InlineElemTag::F32
            }
            (Value::I64(s), InlineElemTag::I64) => {
                for i in 0..n {
                    data[i] = (self.data[i] as i64).wrapping_mul(*s) as u64;
                }
                InlineElemTag::I64
            }
            // Promote: I64 scalar × F64 vector → F64
            (Value::I64(s), InlineElemTag::F64) => {
                let sf = *s as f64;
                for i in 0..n {
                    data[i] = (f64::from_bits(self.data[i]) * sf).to_bits();
                }
                InlineElemTag::F64
            }
            // Promote: F64 scalar × I64 vector → F64
            (Value::F64(s), InlineElemTag::I64) => {
                for i in 0..n {
                    data[i] = ((self.data[i] as i64) as f64 * s).to_bits();
                }
                InlineElemTag::F64
            }
            _ => return None,
        };
        let result = if self.is_vector() {
            Self::new_vector_raw(self.rows as usize, tag, data)
        } else {
            Self::new_matrix_raw(self.rows as usize, self.cols as usize, tag, data)
        };
        Some(Value::StaticArrayInline(result))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Phase 2: Rust arithmetic kernels (Issue #7964)
// ──────────────────────────────────────────────────────────────────────────────

/// Extract the element-type name from a StaticArray type name.
/// `"SVector{2, Float64}"` → `"Float64"`
/// `"SMatrix{2, 2, Float64}"` → `"Float64"`
fn elem_type_str(type_name: &str) -> &str {
    type_name
        .rsplit(", ")
        .next()
        .unwrap_or("")
        .trim_end_matches('}')
}

/// Build a `SVector{N, T}` type-name string without allocating for the common
/// cases we care about (N ≤ 4, T ∈ {Float64, Float32, Int64, Int32, ...}).
/// Falls back to `format!` for unusual combinations.
fn svector_type_name(rows: usize, et: &str) -> Box<str> {
    // Static strings for the hot path (no heap allocation for the string itself,
    // but Box<str> always copies into a heap allocation — the win is avoiding
    // format! overhead, not the Box copy).
    let s: &str = match (rows, et) {
        (1, "Float64") => "SVector{1, Float64}",
        (2, "Float64") => "SVector{2, Float64}",
        (3, "Float64") => "SVector{3, Float64}",
        (4, "Float64") => "SVector{4, Float64}",
        (1, "Float32") => "SVector{1, Float32}",
        (2, "Float32") => "SVector{2, Float32}",
        (3, "Float32") => "SVector{3, Float32}",
        (4, "Float32") => "SVector{4, Float32}",
        (1, "Int64") => "SVector{1, Int64}",
        (2, "Int64") => "SVector{2, Int64}",
        (3, "Int64") => "SVector{3, Int64}",
        (4, "Int64") => "SVector{4, Int64}",
        (1, "Int32") => "SVector{1, Int32}",
        (2, "Int32") => "SVector{2, Int32}",
        (3, "Int32") => "SVector{3, Int32}",
        (4, "Int32") => "SVector{4, Int32}",
        _ => return format!("SVector{{{}, {}}}", rows, et).into(),
    };
    Box::from(s)
}

fn smatrix_type_name(rows: usize, cols: usize, et: &str) -> Box<str> {
    let s: &str = match (rows, cols, et) {
        (2, 2, "Float64") => "SMatrix{2, 2, Float64}",
        (3, 3, "Float64") => "SMatrix{3, 3, Float64}",
        (4, 4, "Float64") => "SMatrix{4, 4, Float64}",
        (2, 2, "Float32") => "SMatrix{2, 2, Float32}",
        (3, 3, "Float32") => "SMatrix{3, 3, Float32}",
        (2, 2, "Int64") => "SMatrix{2, 2, Int64}",
        (3, 3, "Int64") => "SMatrix{3, 3, Int64}",
        _ => return format!("SMatrix{{{}, {}, {}}}", rows, cols, et).into(),
    };
    Box::from(s)
}

// ── Element-wise add/sub ──────────────────────────────────────────────────────

macro_rules! elem_add_arm {
    ($v:ident, $a:ident, $b:ident) => {
        if $a.len() == $b.len() {
            Some(StaticElem::$v(
                $a.iter()
                    .zip($b.iter())
                    .map(|(&x, &y)| x.wrapping_add(y))
                    .collect(),
            ))
        } else {
            None
        }
    };
}

macro_rules! elem_float_add_arm {
    ($v:ident, $a:ident, $b:ident) => {
        if $a.len() == $b.len() {
            Some(StaticElem::$v(
                $a.iter().zip($b.iter()).map(|(&x, &y)| x + y).collect(),
            ))
        } else {
            None
        }
    };
}

macro_rules! elem_sub_arm {
    ($v:ident, $a:ident, $b:ident) => {
        if $a.len() == $b.len() {
            Some(StaticElem::$v(
                $a.iter()
                    .zip($b.iter())
                    .map(|(&x, &y)| x.wrapping_sub(y))
                    .collect(),
            ))
        } else {
            None
        }
    };
}

macro_rules! elem_float_sub_arm {
    ($v:ident, $a:ident, $b:ident) => {
        if $a.len() == $b.len() {
            Some(StaticElem::$v(
                $a.iter().zip($b.iter()).map(|(&x, &y)| x - y).collect(),
            ))
        } else {
            None
        }
    };
}

fn elem_add(a: &StaticElem, b: &StaticElem) -> Option<StaticElem> {
    match (a, b) {
        (StaticElem::F64(a), StaticElem::F64(b)) => elem_float_add_arm!(F64, a, b),
        (StaticElem::F32(a), StaticElem::F32(b)) => elem_float_add_arm!(F32, a, b),
        (StaticElem::I64(a), StaticElem::I64(b)) => elem_add_arm!(I64, a, b),
        (StaticElem::I32(a), StaticElem::I32(b)) => elem_add_arm!(I32, a, b),
        (StaticElem::I16(a), StaticElem::I16(b)) => elem_add_arm!(I16, a, b),
        (StaticElem::I8(a), StaticElem::I8(b)) => elem_add_arm!(I8, a, b),
        (StaticElem::U64(a), StaticElem::U64(b)) => elem_add_arm!(U64, a, b),
        (StaticElem::U32(a), StaticElem::U32(b)) => elem_add_arm!(U32, a, b),
        (StaticElem::U16(a), StaticElem::U16(b)) => elem_add_arm!(U16, a, b),
        (StaticElem::U8(a), StaticElem::U8(b)) => elem_add_arm!(U8, a, b),
        _ => None,
    }
}

fn elem_sub(a: &StaticElem, b: &StaticElem) -> Option<StaticElem> {
    match (a, b) {
        (StaticElem::F64(a), StaticElem::F64(b)) => elem_float_sub_arm!(F64, a, b),
        (StaticElem::F32(a), StaticElem::F32(b)) => elem_float_sub_arm!(F32, a, b),
        (StaticElem::I64(a), StaticElem::I64(b)) => elem_sub_arm!(I64, a, b),
        (StaticElem::I32(a), StaticElem::I32(b)) => elem_sub_arm!(I32, a, b),
        (StaticElem::I16(a), StaticElem::I16(b)) => elem_sub_arm!(I16, a, b),
        (StaticElem::I8(a), StaticElem::I8(b)) => elem_sub_arm!(I8, a, b),
        (StaticElem::U64(a), StaticElem::U64(b)) => elem_sub_arm!(U64, a, b),
        (StaticElem::U32(a), StaticElem::U32(b)) => elem_sub_arm!(U32, a, b),
        (StaticElem::U16(a), StaticElem::U16(b)) => elem_sub_arm!(U16, a, b),
        (StaticElem::U8(a), StaticElem::U8(b)) => elem_sub_arm!(U8, a, b),
        _ => None,
    }
}

// ── Column-major matrix kernels ───────────────────────────────────────────────
// A is (rows × k) stored column-major: A[i,j] = a[i + j*rows]  (0-indexed)
// x is length k.  y[i] = Σ_j A[i,j]*x[j].

macro_rules! matvec_kernel {
    ($T:ty, $rows:expr, $k:expr, $a:expr, $x:expr) => {{
        let mut y = vec![<$T as Default>::default(); $rows];
        for j in 0..$k {
            let xj = $x[j];
            for i in 0..$rows {
                y[i] = y[i].wrapping_add($a[i + j * $rows].wrapping_mul(xj));
            }
        }
        y
    }};
}

macro_rules! matvec_float_kernel {
    ($T:ty, $rows:expr, $k:expr, $a:expr, $x:expr) => {{
        let mut y = vec![0 as $T; $rows];
        for j in 0..$k {
            let xj = $x[j];
            for i in 0..$rows {
                y[i] += $a[i + j * $rows] * xj;
            }
        }
        y
    }};
}

macro_rules! matmat_kernel {
    ($T:ty, $ar:expr, $k:expr, $bc:expr, $a:expr, $b:expr) => {{
        let mut c = vec![<$T as Default>::default(); $ar * $bc];
        for j in 0..$bc {
            for l in 0..$k {
                let blj = $b[l + j * $k];
                for i in 0..$ar {
                    c[i + j * $ar] = c[i + j * $ar].wrapping_add($a[i + l * $ar].wrapping_mul(blj));
                }
            }
        }
        c
    }};
}

macro_rules! matmat_float_kernel {
    ($T:ty, $ar:expr, $k:expr, $bc:expr, $a:expr, $b:expr) => {{
        let mut c = vec![0 as $T; $ar * $bc];
        for j in 0..$bc {
            for l in 0..$k {
                let blj = $b[l + j * $k];
                for i in 0..$ar {
                    c[i + j * $ar] += $a[i + l * $ar] * blj;
                }
            }
        }
        c
    }};
}

// ── Public arithmetic API ─────────────────────────────────────────────────────

/// `SVector{N,T} + SVector{N,T}` or `SMatrix + SMatrix` (same shape, same T).
pub fn static_add(a: &StaticRealValue, b: &StaticRealValue) -> Option<Value> {
    if a.rows != b.rows || a.cols != b.cols {
        return None;
    }
    let elems = elem_add(&a.elems, &b.elems)?;
    Some(Value::StaticArray(Box::new(if a.is_vector() {
        StaticRealValue::new_vector(a.type_name.clone(), elems)
    } else {
        StaticRealValue::new_matrix(a.type_name.clone(), a.rows, a.cols, elems)
    })))
}

/// `SVector{N,T} - SVector{N,T}` or `SMatrix - SMatrix` (same shape, same T).
pub fn static_sub(a: &StaticRealValue, b: &StaticRealValue) -> Option<Value> {
    if a.rows != b.rows || a.cols != b.cols {
        return None;
    }
    let elems = elem_sub(&a.elems, &b.elems)?;
    Some(Value::StaticArray(Box::new(if a.is_vector() {
        StaticRealValue::new_vector(a.type_name.clone(), elems)
    } else {
        StaticRealValue::new_matrix(a.type_name.clone(), a.rows, a.cols, elems)
    })))
}

/// `SMatrix{M,K,T} * SVector{K,T}` → `SVector{M,T}`.
pub fn static_matvec(mat: &StaticRealValue, vec: &StaticRealValue) -> Option<Value> {
    if mat.cols != vec.rows || !vec.is_vector() {
        return None;
    }
    let (m, k) = (mat.rows, mat.cols);
    let et = elem_type_str(&mat.type_name);
    let result_elems = match (&mat.elems, &vec.elems) {
        (StaticElem::F64(a), StaticElem::F64(x)) => {
            StaticElem::F64(matvec_float_kernel!(f64, m, k, a, x))
        }
        (StaticElem::F32(a), StaticElem::F32(x)) => {
            StaticElem::F32(matvec_float_kernel!(f32, m, k, a, x))
        }
        (StaticElem::I64(a), StaticElem::I64(x)) => {
            StaticElem::I64(matvec_kernel!(i64, m, k, a, x))
        }
        (StaticElem::I32(a), StaticElem::I32(x)) => {
            StaticElem::I32(matvec_kernel!(i32, m, k, a, x))
        }
        (StaticElem::I16(a), StaticElem::I16(x)) => {
            StaticElem::I16(matvec_kernel!(i16, m, k, a, x))
        }
        (StaticElem::I8(a), StaticElem::I8(x)) => StaticElem::I8(matvec_kernel!(i8, m, k, a, x)),
        (StaticElem::U64(a), StaticElem::U64(x)) => {
            StaticElem::U64(matvec_kernel!(u64, m, k, a, x))
        }
        (StaticElem::U32(a), StaticElem::U32(x)) => {
            StaticElem::U32(matvec_kernel!(u32, m, k, a, x))
        }
        (StaticElem::U16(a), StaticElem::U16(x)) => {
            StaticElem::U16(matvec_kernel!(u16, m, k, a, x))
        }
        (StaticElem::U8(a), StaticElem::U8(x)) => StaticElem::U8(matvec_kernel!(u8, m, k, a, x)),
        _ => return None,
    };
    Some(Value::StaticArray(Box::new(StaticRealValue::new_vector(
        svector_type_name(m, et),
        result_elems,
    ))))
}

/// `SMatrix{M,K,T} * SMatrix{K,N,T}` → `SMatrix{M,N,T}`.
pub fn static_matmat(a: &StaticRealValue, b: &StaticRealValue) -> Option<Value> {
    if a.cols != b.rows || a.is_vector() || b.is_vector() {
        return None;
    }
    let (ar, k, bc) = (a.rows, a.cols, b.cols);
    let et = elem_type_str(&a.type_name);
    let result_elems = match (&a.elems, &b.elems) {
        (StaticElem::F64(ae), StaticElem::F64(be)) => {
            StaticElem::F64(matmat_float_kernel!(f64, ar, k, bc, ae, be))
        }
        (StaticElem::F32(ae), StaticElem::F32(be)) => {
            StaticElem::F32(matmat_float_kernel!(f32, ar, k, bc, ae, be))
        }
        (StaticElem::I64(ae), StaticElem::I64(be)) => {
            StaticElem::I64(matmat_kernel!(i64, ar, k, bc, ae, be))
        }
        (StaticElem::I32(ae), StaticElem::I32(be)) => {
            StaticElem::I32(matmat_kernel!(i32, ar, k, bc, ae, be))
        }
        (StaticElem::I16(ae), StaticElem::I16(be)) => {
            StaticElem::I16(matmat_kernel!(i16, ar, k, bc, ae, be))
        }
        (StaticElem::I8(ae), StaticElem::I8(be)) => {
            StaticElem::I8(matmat_kernel!(i8, ar, k, bc, ae, be))
        }
        (StaticElem::U64(ae), StaticElem::U64(be)) => {
            StaticElem::U64(matmat_kernel!(u64, ar, k, bc, ae, be))
        }
        (StaticElem::U32(ae), StaticElem::U32(be)) => {
            StaticElem::U32(matmat_kernel!(u32, ar, k, bc, ae, be))
        }
        (StaticElem::U16(ae), StaticElem::U16(be)) => {
            StaticElem::U16(matmat_kernel!(u16, ar, k, bc, ae, be))
        }
        (StaticElem::U8(ae), StaticElem::U8(be)) => {
            StaticElem::U8(matmat_kernel!(u8, ar, k, bc, ae, be))
        }
        _ => return None,
    };
    Some(Value::StaticArray(Box::new(StaticRealValue::new_matrix(
        smatrix_type_name(ar, bc, et),
        ar,
        bc,
        result_elems,
    ))))
}

/// `scalar * SVector/SMatrix` or `SVector/SMatrix * scalar`.
/// Handles same-type and common mixed-type (I64×F64, F64×I64) promotions.
pub fn static_scalar_mul(scalar: &Value, sv: &StaticRealValue) -> Option<Value> {
    let result_elems = match (scalar, &sv.elems) {
        (Value::F64(s), StaticElem::F64(v)) => StaticElem::F64(v.iter().map(|&x| x * s).collect()),
        (Value::F32(s), StaticElem::F32(v)) => StaticElem::F32(v.iter().map(|&x| x * s).collect()),
        (Value::I64(s), StaticElem::I64(v)) => {
            StaticElem::I64(v.iter().map(|&x| x.wrapping_mul(*s)).collect())
        }
        (Value::I32(s), StaticElem::I32(v)) => {
            StaticElem::I32(v.iter().map(|&x| x.wrapping_mul(*s)).collect())
        }
        (Value::I16(s), StaticElem::I16(v)) => {
            StaticElem::I16(v.iter().map(|&x| x.wrapping_mul(*s)).collect())
        }
        (Value::I8(s), StaticElem::I8(v)) => {
            StaticElem::I8(v.iter().map(|&x| x.wrapping_mul(*s)).collect())
        }
        (Value::U64(s), StaticElem::U64(v)) => {
            StaticElem::U64(v.iter().map(|&x| x.wrapping_mul(*s)).collect())
        }
        (Value::U32(s), StaticElem::U32(v)) => {
            StaticElem::U32(v.iter().map(|&x| x.wrapping_mul(*s)).collect())
        }
        (Value::U16(s), StaticElem::U16(v)) => {
            StaticElem::U16(v.iter().map(|&x| x.wrapping_mul(*s)).collect())
        }
        (Value::U8(s), StaticElem::U8(v)) => {
            StaticElem::U8(v.iter().map(|&x| x.wrapping_mul(*s)).collect())
        }
        // promote: I64 scalar × F64 vector → F64
        (Value::I64(s), StaticElem::F64(v)) => {
            StaticElem::F64(v.iter().map(|&x| x * *s as f64).collect())
        }
        // promote: F64 scalar × I64 vector → F64
        (Value::F64(s), StaticElem::I64(v)) => {
            StaticElem::F64(v.iter().map(|&x| x as f64 * s).collect())
        }
        _ => return None,
    };
    // Reuse the existing type_name when the element type is unchanged;
    // rebuild only for promoting I64↔F64 cases (rare).
    let result_type_name: Box<str> = match (scalar, &sv.elems) {
        (Value::I64(_), StaticElem::F64(_)) | (Value::F64(_), StaticElem::I64(_)) => {
            let et = elem_type_str(&sv.type_name);
            sv.type_name.replace(et, "Float64").into()
        }
        _ => sv.type_name.clone(),
    };
    Some(Value::StaticArray(Box::new(if sv.is_vector() {
        StaticRealValue::new_vector(result_type_name, result_elems)
    } else {
        StaticRealValue::new_matrix(result_type_name, sv.rows, sv.cols, result_elems)
    })))
}
