//! ArrayData - Type-segregated array storage for efficient operations.
//!
//! This module contains the `ArrayData` enum which holds homogeneous vectors
//! for each supported element type.

// SAFETY: i64→u8/u16/u32/u64 casts are all guarded by `if x >= 0` match guards
// (pattern `Value::I64(x) if x >= 0`) before the cast occurs.
#![allow(clippy::cast_sign_loss)]

use serde::{Deserialize, Serialize};

use super::super::error::VmError;
use super::array_element::ArrayElementType;
use super::Value;

const BITS_PER_WORD: usize = u64::BITS as usize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitPackedBoolData {
    words: Vec<u64>,
    len: usize,
}

impl BitPackedBoolData {
    pub fn new_false(len: usize) -> Self {
        Self {
            words: vec![0; Self::words_for_len(len)],
            len,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            words: Vec::with_capacity(Self::words_for_len(capacity)),
            len: 0,
        }
    }

    pub fn from_bools(values: &[bool]) -> Self {
        let mut packed = Self::with_capacity(values.len());
        for &value in values {
            packed.push(value);
        }
        packed
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn raw_word_len(&self) -> usize {
        self.words.len()
    }

    pub fn reserve(&mut self, additional: usize) {
        let required_words = Self::words_for_len(self.len.saturating_add(additional));
        self.words
            .reserve(required_words.saturating_sub(self.words.capacity()));
    }

    pub fn get(&self, index: usize) -> Option<bool> {
        if index >= self.len {
            return None;
        }
        let (word_idx, mask) = Self::word_mask(index);
        self.words.get(word_idx).map(|word| (word & mask) != 0)
    }

    pub fn set(&mut self, index: usize, value: bool) -> Option<()> {
        if index >= self.len {
            return None;
        }
        let (word_idx, mask) = Self::word_mask(index);
        let word = self.words.get_mut(word_idx)?;
        if value {
            *word |= mask;
        } else {
            *word &= !mask;
        }
        Some(())
    }

    pub fn push(&mut self, value: bool) {
        if self.len == self.words.len() * BITS_PER_WORD {
            self.words.push(0);
        }
        self.len += 1;
        let _ = self.set(self.len - 1, value);
    }

    pub fn pop(&mut self) -> Option<bool> {
        if self.len == 0 {
            return None;
        }
        let index = self.len - 1;
        let value = self.get(index)?;
        self.len -= 1;
        if self.len == 0 {
            self.words.clear();
        } else {
            self.words.truncate(Self::words_for_len(self.len));
            let unused_bits = self.words.len() * BITS_PER_WORD - self.len;
            if unused_bits > 0 {
                let keep_bits = BITS_PER_WORD - unused_bits;
                if let Some(last) = self.words.last_mut() {
                    *last &= (1_u64 << keep_bits) - 1;
                }
            }
        }
        Some(value)
    }

    pub fn insert(&mut self, index: usize, value: bool) -> Option<()> {
        if index > self.len {
            return None;
        }
        self.push(false);
        for i in (index + 1..self.len).rev() {
            let prev = self.get(i - 1)?;
            self.set(i, prev)?;
        }
        self.set(index, value)
    }

    pub fn remove(&mut self, index: usize) -> Option<bool> {
        if index >= self.len {
            return None;
        }
        let removed = self.get(index)?;
        for i in index..self.len - 1 {
            let next = self.get(i + 1)?;
            self.set(i, next)?;
        }
        let _ = self.pop();
        Some(removed)
    }

    pub fn iter(&self) -> impl Iterator<Item = bool> + '_ {
        (0..self.len).map(|index| self.get(index).unwrap_or(false))
    }

    fn words_for_len(len: usize) -> usize {
        len.div_ceil(BITS_PER_WORD)
    }

    fn word_mask(index: usize) -> (usize, u64) {
        let word_idx = index / BITS_PER_WORD;
        let bit_idx = index % BITS_PER_WORD;
        (word_idx, 1_u64 << bit_idx)
    }
}

/// Type-segregated array storage for efficient operations
/// Each variant holds a homogeneous vector of the corresponding type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArrayData {
    // Floating point types
    F32(Vec<f32>),
    F64(Vec<f64>),
    // Signed integer types
    I8(Vec<i8>),
    I16(Vec<i16>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    // Unsigned integer types
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
    U64(Vec<u64>),
    // Other types
    Bool(Vec<bool>),
    BitPackedBool(BitPackedBoolData),
    String(Vec<Value>),
    Char(Vec<char>),
    StructRefs(Vec<usize>),
    Any(Vec<Value>),
    /// Contiguous, byte-unboxed storage for arrays of an all-`Float64` isbits
    /// immutable struct (Issue #9198 S4). Fields are stored interleaved AoS
    /// as raw `f64` (`[f0_1, f1_1, …, f0_2, f1_2, …]`), with `field_count`
    /// carried by the companion `ArrayElementType::StructInlineF64(type_id,
    /// field_count)` override (mirrors how `ArrayData::F64` backs the
    /// `ComplexF64` interleaved layout). This is the general form of upstream
    /// `jl_get_genericmemory_layout` case 1 for the 2×f64 SROA'd shape
    /// (`Complex{Float64}`, user `struct V2 x::Float64; y::Float64 end`).
    /// Declared LAST for append-only enum hygiene; `ArrayData` is never
    /// bincode-serialized (see `value_enum.rs` `SerializableValue`), so this
    /// carries no wire-format impact.
    StructF64(Vec<f64>),
}

impl ArrayData {
    /// Re-box unboxed `Vec<char>` storage as `Any` storage so a malformed
    /// Char (Issue #8995) can be stored. Element values are unchanged.
    fn promote_char_storage_to_any(&mut self) {
        if let ArrayData::Char(v) = self {
            let values: Vec<Value> = std::mem::take(v).into_iter().map(Value::Char).collect();
            *self = ArrayData::Any(values);
        }
    }

    /// Get the element type of this data
    pub fn element_type(&self) -> ArrayElementType {
        match self {
            ArrayData::F32(_) => ArrayElementType::F32,
            ArrayData::F64(_) => ArrayElementType::F64,
            ArrayData::I8(_) => ArrayElementType::I8,
            ArrayData::I16(_) => ArrayElementType::I16,
            ArrayData::I32(_) => ArrayElementType::I32,
            ArrayData::I64(_) => ArrayElementType::I64,
            ArrayData::U8(_) => ArrayElementType::U8,
            ArrayData::U16(_) => ArrayElementType::U16,
            ArrayData::U32(_) => ArrayElementType::U32,
            ArrayData::U64(_) => ArrayElementType::U64,
            ArrayData::Bool(_) => ArrayElementType::Bool,
            ArrayData::BitPackedBool(_) => ArrayElementType::Bool,
            ArrayData::String(_) => ArrayElementType::String,
            ArrayData::Char(_) => ArrayElementType::Char,
            ArrayData::StructRefs(_) => ArrayElementType::Struct,
            ArrayData::Any(_) => ArrayElementType::Any,
            // Raw storage is a flat f64 buffer; the logical isbits-struct
            // element type is carried by the `StructInlineF64` override, which
            // is always present for this variant (Issue #9198 S4).
            ArrayData::StructF64(_) => ArrayElementType::F64,
        }
    }

    /// Get the raw length (number of stored elements)
    pub fn raw_len(&self) -> usize {
        match self {
            ArrayData::F32(v) => v.len(),
            ArrayData::F64(v) => v.len(),
            ArrayData::I8(v) => v.len(),
            ArrayData::I16(v) => v.len(),
            ArrayData::I32(v) => v.len(),
            ArrayData::I64(v) => v.len(),
            ArrayData::U8(v) => v.len(),
            ArrayData::U16(v) => v.len(),
            ArrayData::U32(v) => v.len(),
            ArrayData::U64(v) => v.len(),
            ArrayData::Bool(v) => v.len(),
            ArrayData::BitPackedBool(v) => v.len(),
            ArrayData::String(v) => v.len(),
            ArrayData::Char(v) => v.len(),
            ArrayData::StructRefs(v) => v.len(),
            ArrayData::Any(v) => v.len(),
            ArrayData::StructF64(v) => v.len(),
        }
    }

    /// Reserve capacity for at least `additional` more *raw* slots in the
    /// backing vector, leaving the logical length unchanged (Issue #5186).
    ///
    /// This is a pure capacity hint used to pre-size the backing storage of a
    /// filter-free comprehension whose final length is known at runtime, so the
    /// repeated `push` growth no longer triggers `O(log n)` reallocations. The
    /// caller is responsible for scaling `additional` by the per-element raw
    /// multiplier (e.g. 2 for interleaved Complex storage, the field count for
    /// AoS Tuple/struct storage).
    pub fn reserve(&mut self, additional: usize) {
        match self {
            ArrayData::F32(v) => v.reserve(additional),
            ArrayData::F64(v) => v.reserve(additional),
            ArrayData::I8(v) => v.reserve(additional),
            ArrayData::I16(v) => v.reserve(additional),
            ArrayData::I32(v) => v.reserve(additional),
            ArrayData::I64(v) => v.reserve(additional),
            ArrayData::U8(v) => v.reserve(additional),
            ArrayData::U16(v) => v.reserve(additional),
            ArrayData::U32(v) => v.reserve(additional),
            ArrayData::U64(v) => v.reserve(additional),
            ArrayData::Bool(v) => v.reserve(additional),
            ArrayData::BitPackedBool(v) => v.reserve(additional),
            ArrayData::String(v) => v.reserve(additional),
            ArrayData::Char(v) => v.reserve(additional),
            ArrayData::StructRefs(v) => v.reserve(additional),
            ArrayData::Any(v) => v.reserve(additional),
            ArrayData::StructF64(v) => v.reserve(additional),
        }
    }

    /// Check if the data is empty
    pub fn is_empty(&self) -> bool {
        match self {
            ArrayData::F32(v) => v.is_empty(),
            ArrayData::F64(v) => v.is_empty(),
            ArrayData::I8(v) => v.is_empty(),
            ArrayData::I16(v) => v.is_empty(),
            ArrayData::I32(v) => v.is_empty(),
            ArrayData::I64(v) => v.is_empty(),
            ArrayData::U8(v) => v.is_empty(),
            ArrayData::U16(v) => v.is_empty(),
            ArrayData::U32(v) => v.is_empty(),
            ArrayData::U64(v) => v.is_empty(),
            ArrayData::Bool(v) => v.is_empty(),
            ArrayData::BitPackedBool(v) => v.is_empty(),
            ArrayData::String(v) => v.is_empty(),
            ArrayData::Char(v) => v.is_empty(),
            ArrayData::StructRefs(v) => v.is_empty(),
            ArrayData::Any(v) => v.is_empty(),
            ArrayData::StructF64(v) => v.is_empty(),
        }
    }

    /// Sum all numeric elements as f64
    pub fn sum_as_f64(&self) -> f64 {
        match self {
            ArrayData::F32(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::F64(v) => v.iter().sum(),
            ArrayData::I8(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::I16(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::I32(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::I64(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::U8(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::U16(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::U32(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::U64(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::Bool(v) => v.iter().map(|&x| if x { 1.0 } else { 0.0 }).sum(),
            ArrayData::BitPackedBool(v) => v.iter().map(|x| if x { 1.0 } else { 0.0 }).sum(),
            ArrayData::Any(v) => {
                // Sum boxed numeric values (from collect(map(...)))
                v.iter()
                    .map(|val| match val {
                        Value::I8(x) => *x as f64,
                        Value::I16(x) => *x as f64,
                        Value::I32(x) => *x as f64,
                        Value::I64(x) => *x as f64,
                        Value::U8(x) => *x as f64,
                        Value::U16(x) => *x as f64,
                        Value::U32(x) => *x as f64,
                        Value::U64(x) => *x as f64,
                        Value::F32(x) => *x as f64,
                        Value::F64(x) => *x,
                        Value::Bool(true) => 1.0,
                        Value::Bool(false) => 0.0,
                        _ => 0.0, // Non-numeric values contribute 0
                    })
                    .sum()
            }
            // A struct array is not a numeric aggregate; `sum` dispatches to
            // Julia `+` over the reconstructed struct values, never this fast
            // path (Issue #9198 S4). Reported as 0.0 like the other non-numeric
            // storages.
            ArrayData::String(_)
            | ArrayData::Char(_)
            | ArrayData::StructRefs(_)
            | ArrayData::StructF64(_) => 0.0,
        }
    }

    /// Get a string representation of the type for error messages
    pub fn type_name(&self) -> &'static str {
        match self {
            ArrayData::F32(_) => "F32",
            ArrayData::F64(_) => "F64",
            ArrayData::I8(_) => "I8",
            ArrayData::I16(_) => "I16",
            ArrayData::I32(_) => "I32",
            ArrayData::I64(_) => "I64",
            ArrayData::U8(_) => "U8",
            ArrayData::U16(_) => "U16",
            ArrayData::U32(_) => "U32",
            ArrayData::U64(_) => "U64",
            ArrayData::Bool(_) => "Bool",
            ArrayData::BitPackedBool(_) => "Bool",
            ArrayData::String(_) => "String",
            ArrayData::Char(_) => "Char",
            ArrayData::StructRefs(_) => "StructRefs",
            ArrayData::Any(_) => "Any",
            ArrayData::StructF64(_) => "StructF64",
        }
    }

    /// Get a value at a linear index, converting to Value
    /// For StructRefs, returns Value::StructRef(heap_index)
    pub fn get_value(&self, index: usize) -> Option<Value> {
        match self {
            ArrayData::F32(v) => v.get(index).map(|&x| Value::F32(x)),
            ArrayData::F64(v) => v.get(index).map(|&x| Value::F64(x)),
            ArrayData::I8(v) => v.get(index).map(|&x| Value::I8(x)),
            ArrayData::I16(v) => v.get(index).map(|&x| Value::I16(x)),
            ArrayData::I32(v) => v.get(index).map(|&x| Value::I32(x)),
            ArrayData::I64(v) => v.get(index).map(|&x| Value::I64(x)),
            ArrayData::U8(v) => v.get(index).map(|&x| Value::U8(x)),
            ArrayData::U16(v) => v.get(index).map(|&x| Value::U16(x)),
            ArrayData::U32(v) => v.get(index).map(|&x| Value::U32(x)),
            ArrayData::U64(v) => v.get(index).map(|&x| Value::U64(x)),
            ArrayData::Bool(v) => v.get(index).map(|&x| Value::Bool(x)),
            ArrayData::BitPackedBool(v) => v.get(index).map(Value::Bool),
            ArrayData::String(v) => v.get(index).cloned(),
            ArrayData::Char(v) => v.get(index).map(|&x| Value::Char(x)),
            ArrayData::StructRefs(v) => v.get(index).map(|&idx| Value::StructRef(idx)),
            ArrayData::Any(v) => v.get(index).cloned(),
            // Raw f64 slot; struct reconstruction (grouping `field_count`
            // consecutive slots) is done by the `StructInlineF64` override in
            // `ArrayValue::get_linear_value` (Issue #9198 S4).
            ArrayData::StructF64(v) => v.get(index).map(|&x| Value::F64(x)),
        }
    }

    /// Set a value at a linear index
    pub fn set_value(&mut self, index: usize, value: Value) -> Result<(), VmError> {
        macro_rules! check_bounds {
            ($v:expr) => {
                if index >= $v.len() {
                    return Err(VmError::IndexOutOfBounds {
                        indices: vec![index as i64 + 1],
                        shape: vec![$v.len()],
                    });
                }
            };
        }
        match self {
            ArrayData::F32(v) => {
                check_bounds!(v);
                match value {
                    Value::F32(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::F64(x) => {
                        v[index] = x as f32;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x as f32;
                        Ok(())
                    }
                    Value::I32(x) => {
                        v[index] = x as f32;
                        Ok(())
                    }
                    Value::I16(x) => {
                        v[index] = x as f32;
                        Ok(())
                    }
                    Value::I8(x) => {
                        v[index] = x as f32;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in F32 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::F64(v) => {
                check_bounds!(v);
                match value {
                    Value::F64(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::F32(x) => {
                        v[index] = x as f64;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x as f64;
                        Ok(())
                    }
                    Value::I32(x) => {
                        v[index] = x as f64;
                        Ok(())
                    }
                    Value::I16(x) => {
                        v[index] = x as f64;
                        Ok(())
                    }
                    Value::I8(x) => {
                        v[index] = x as f64;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in F64 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::I8(v) => {
                check_bounds!(v);
                match value {
                    Value::I8(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x as i8;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in I8 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::I16(v) => {
                check_bounds!(v);
                match value {
                    Value::I16(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x as i16;
                        Ok(())
                    }
                    Value::I8(x) => {
                        v[index] = x as i16;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in I16 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::I32(v) => {
                check_bounds!(v);
                match value {
                    Value::I32(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x as i32;
                        Ok(())
                    }
                    Value::I16(x) => {
                        v[index] = x as i32;
                        Ok(())
                    }
                    Value::I8(x) => {
                        v[index] = x as i32;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in I32 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::I64(v) => {
                check_bounds!(v);
                match value {
                    Value::I64(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::F64(x) if x.fract() == 0.0 => {
                        v[index] = x as i64;
                        Ok(())
                    }
                    Value::I32(x) => {
                        v[index] = x as i64;
                        Ok(())
                    }
                    Value::I16(x) => {
                        v[index] = x as i64;
                        Ok(())
                    }
                    Value::I8(x) => {
                        v[index] = x as i64;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in I64 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::U8(v) => {
                check_bounds!(v);
                match value {
                    Value::U8(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) if x >= 0 => {
                        v[index] = x as u8;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in U8 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::U16(v) => {
                check_bounds!(v);
                match value {
                    Value::U16(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) if x >= 0 => {
                        v[index] = x as u16;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in U16 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::U32(v) => {
                check_bounds!(v);
                match value {
                    Value::U32(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) if x >= 0 => {
                        v[index] = x as u32;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in U32 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::U64(v) => {
                check_bounds!(v);
                match value {
                    Value::U64(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) if x >= 0 => {
                        v[index] = x as u64;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in U64 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::Bool(v) => {
                check_bounds!(v);
                match value {
                    Value::Bool(b) => {
                        v[index] = b;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x != 0;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in Bool array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::BitPackedBool(v) => {
                check_bounds!(v);
                match value {
                    Value::Bool(b) => v.set(index, b).ok_or(VmError::IndexOutOfBounds {
                        indices: vec![index as i64 + 1],
                        shape: vec![v.len()],
                    }),
                    Value::I64(x) => v.set(index, x != 0).ok_or(VmError::IndexOutOfBounds {
                        indices: vec![index as i64 + 1],
                        shape: vec![v.len()],
                    }),
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in Bool array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::String(v) => {
                check_bounds!(v);
                match value {
                    Value::Str(_) | Value::StrBytes(_) => {
                        v[index] = value;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in String array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::Char(_) if matches!(value, Value::CharMalformed(_)) => {
                // Promote to boxed storage for malformed Chars (Issue #8995).
                self.promote_char_storage_to_any();
                self.set_value(index, value)
            }
            ArrayData::Char(v) => {
                check_bounds!(v);
                match value {
                    Value::Char(c) => {
                        v[index] = c;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in Char array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::StructRefs(v) => {
                check_bounds!(v);
                match value {
                    Value::StructRef(idx) => {
                        v[index] = idx;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in StructRefs array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::Any(v) => {
                check_bounds!(v);
                v[index] = value;
                Ok(())
            }
            // Raw f64 slot write; struct-field packing (grouping `field_count`
            // consecutive slots) is handled by the `StructInlineF64` override
            // in `ArrayValue::set_linear_value` (Issue #9198 S4).
            ArrayData::StructF64(v) => {
                check_bounds!(v);
                match value {
                    Value::F64(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::F32(x) => {
                        v[index] = x as f64;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x as f64;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in StructF64 array",
                        value.value_type()
                    ))),
                }
            }
        }
    }

    /// Push a value to the end (for 1D arrays)
    pub fn push_value(&mut self, value: Value) -> Result<(), VmError> {
        match self {
            ArrayData::F32(v) => match value {
                Value::F32(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::F64(x) => {
                    v.push(x as f32);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x as f32);
                    Ok(())
                }
                Value::I32(x) => {
                    v.push(x as f32);
                    Ok(())
                }
                Value::I16(x) => {
                    v.push(x as f32);
                    Ok(())
                }
                Value::I8(x) => {
                    v.push(x as f32);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to F32 array",
                    value.value_type()
                ))),
            },
            ArrayData::F64(v) => match value {
                Value::F64(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::F32(x) => {
                    v.push(x as f64);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x as f64);
                    Ok(())
                }
                Value::I32(x) => {
                    v.push(x as f64);
                    Ok(())
                }
                Value::I16(x) => {
                    v.push(x as f64);
                    Ok(())
                }
                Value::I8(x) => {
                    v.push(x as f64);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to F64 array",
                    value.value_type()
                ))),
            },
            ArrayData::I8(v) => match value {
                Value::I8(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x as i8);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to I8 array",
                    value.value_type()
                ))),
            },
            ArrayData::I16(v) => match value {
                Value::I16(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x as i16);
                    Ok(())
                }
                Value::I8(x) => {
                    v.push(x as i16);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to I16 array",
                    value.value_type()
                ))),
            },
            ArrayData::I32(v) => match value {
                Value::I32(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x as i32);
                    Ok(())
                }
                Value::I16(x) => {
                    v.push(x as i32);
                    Ok(())
                }
                Value::I8(x) => {
                    v.push(x as i32);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to I32 array",
                    value.value_type()
                ))),
            },
            ArrayData::I64(v) => match value {
                Value::I64(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::F64(x) if x.fract() == 0.0 => {
                    v.push(x as i64);
                    Ok(())
                }
                Value::I32(x) => {
                    v.push(x as i64);
                    Ok(())
                }
                Value::I16(x) => {
                    v.push(x as i64);
                    Ok(())
                }
                Value::I8(x) => {
                    v.push(x as i64);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to I64 array",
                    value.value_type()
                ))),
            },
            ArrayData::U8(v) => match value {
                Value::U8(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) if x >= 0 => {
                    v.push(x as u8);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to U8 array",
                    value.value_type()
                ))),
            },
            ArrayData::U16(v) => match value {
                Value::U16(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) if x >= 0 => {
                    v.push(x as u16);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to U16 array",
                    value.value_type()
                ))),
            },
            ArrayData::U32(v) => match value {
                Value::U32(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) if x >= 0 => {
                    v.push(x as u32);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to U32 array",
                    value.value_type()
                ))),
            },
            ArrayData::U64(v) => match value {
                Value::U64(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) if x >= 0 => {
                    v.push(x as u64);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to U64 array",
                    value.value_type()
                ))),
            },
            ArrayData::Bool(v) => match value {
                Value::Bool(b) => {
                    v.push(b);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x != 0);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to Bool array",
                    value.value_type()
                ))),
            },
            ArrayData::BitPackedBool(v) => match value {
                Value::Bool(b) => {
                    v.push(b);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x != 0);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to Bool array",
                    value.value_type()
                ))),
            },
            ArrayData::String(v) => match value {
                Value::Str(_) | Value::StrBytes(_) => {
                    v.push(value);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to String array",
                    value.value_type()
                ))),
            },
            // A malformed Char (Issue #8995) cannot live in the unboxed
            // `Vec<char>` storage; promote to boxed `Any` storage first (the
            // Julia-visible element values are unchanged).
            ArrayData::Char(_) if matches!(value, Value::CharMalformed(_)) => {
                self.promote_char_storage_to_any();
                self.push_value(value)
            }
            ArrayData::Char(v) => match value {
                Value::Char(c) => {
                    v.push(c);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to Char array",
                    value.value_type()
                ))),
            },
            ArrayData::StructRefs(v) => match value {
                Value::StructRef(idx) => {
                    v.push(idx);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to StructRefs array",
                    value.value_type()
                ))),
            },
            ArrayData::Any(v) => {
                v.push(value);
                Ok(())
            }
            // Raw f64 slot push; struct-field packing is handled by the
            // `StructInlineF64` arm of `push_into_array_data` (Issue #9198 S4).
            ArrayData::StructF64(v) => match value {
                Value::F64(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::F32(x) => {
                    v.push(x as f64);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x as f64);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to StructF64 array",
                    value.value_type()
                ))),
            },
        }
    }

    /// Pop a value from the end (for 1D arrays)
    pub fn pop_value(&mut self) -> Result<Value, VmError> {
        match self {
            ArrayData::F32(v) => v.pop().map(Value::F32).ok_or(VmError::EmptyArrayPop),
            ArrayData::F64(v) => v.pop().map(Value::F64).ok_or(VmError::EmptyArrayPop),
            ArrayData::I8(v) => v.pop().map(Value::I8).ok_or(VmError::EmptyArrayPop),
            ArrayData::I16(v) => v.pop().map(Value::I16).ok_or(VmError::EmptyArrayPop),
            ArrayData::I32(v) => v.pop().map(Value::I32).ok_or(VmError::EmptyArrayPop),
            ArrayData::I64(v) => v.pop().map(Value::I64).ok_or(VmError::EmptyArrayPop),
            ArrayData::U8(v) => v.pop().map(Value::U8).ok_or(VmError::EmptyArrayPop),
            ArrayData::U16(v) => v.pop().map(Value::U16).ok_or(VmError::EmptyArrayPop),
            ArrayData::U32(v) => v.pop().map(Value::U32).ok_or(VmError::EmptyArrayPop),
            ArrayData::U64(v) => v.pop().map(Value::U64).ok_or(VmError::EmptyArrayPop),
            ArrayData::Bool(v) => v.pop().map(Value::Bool).ok_or(VmError::EmptyArrayPop),
            ArrayData::BitPackedBool(v) => v.pop().map(Value::Bool).ok_or(VmError::EmptyArrayPop),
            ArrayData::String(v) => v.pop().ok_or(VmError::EmptyArrayPop),
            ArrayData::Char(v) => v.pop().map(Value::Char).ok_or(VmError::EmptyArrayPop),
            ArrayData::StructRefs(v) => v.pop().map(Value::StructRef).ok_or(VmError::EmptyArrayPop),
            ArrayData::Any(v) => v.pop().ok_or(VmError::EmptyArrayPop),
            ArrayData::StructF64(v) => v.pop().map(Value::F64).ok_or(VmError::EmptyArrayPop),
        }
    }

    /// Get a reference to the underlying f64 data (for backward compatibility)
    /// Returns None if not an F64 array
    pub fn as_f64_slice(&self) -> Option<&[f64]> {
        match self {
            ArrayData::F64(v) => Some(v),
            _ => None,
        }
    }

    /// Get a mutable reference to the underlying f64 data (for backward compatibility)
    pub fn as_f64_slice_mut(&mut self) -> Option<&mut Vec<f64>> {
        match self {
            ArrayData::F64(v) => Some(v),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::array_element::ArrayElementType;

    // ── element_type ──────────────────────────────────────────────────────────

    #[test]
    fn test_element_type_f64() {
        assert_eq!(
            ArrayData::F64(vec![1.0]).element_type(),
            ArrayElementType::F64
        );
    }

    #[test]
    fn test_element_type_i64() {
        assert_eq!(
            ArrayData::I64(vec![1]).element_type(),
            ArrayElementType::I64
        );
    }

    #[test]
    fn test_element_type_bool() {
        assert_eq!(
            ArrayData::Bool(vec![true]).element_type(),
            ArrayElementType::Bool
        );
    }

    #[test]
    fn test_element_type_bitpacked_bool() {
        assert_eq!(
            ArrayData::BitPackedBool(BitPackedBoolData::new_false(3)).element_type(),
            ArrayElementType::Bool
        );
    }

    #[test]
    fn test_element_type_any() {
        assert_eq!(ArrayData::Any(vec![]).element_type(), ArrayElementType::Any);
    }

    // ── raw_len / is_empty ────────────────────────────────────────────────────

    #[test]
    fn test_raw_len_f64() {
        assert_eq!(ArrayData::F64(vec![1.0, 2.0, 3.0]).raw_len(), 3);
    }

    #[test]
    fn test_raw_len_empty() {
        assert_eq!(ArrayData::I64(vec![]).raw_len(), 0);
    }

    #[test]
    fn test_bitpacked_bool_raw_len_is_logical_len() {
        let data = BitPackedBoolData::from_bools(&[true, false, true, true, false]);
        assert_eq!(ArrayData::BitPackedBool(data).raw_len(), 5);
    }

    #[test]
    fn test_is_empty_true() {
        assert!(ArrayData::F64(vec![]).is_empty());
    }

    #[test]
    fn test_is_empty_false() {
        assert!(!ArrayData::I64(vec![42]).is_empty());
    }

    // ── type_name ─────────────────────────────────────────────────────────────

    #[test]
    fn test_type_name_f64() {
        assert_eq!(ArrayData::F64(vec![]).type_name(), "F64");
    }

    #[test]
    fn test_type_name_i64() {
        assert_eq!(ArrayData::I64(vec![]).type_name(), "I64");
    }

    #[test]
    fn test_type_name_bool() {
        assert_eq!(ArrayData::Bool(vec![]).type_name(), "Bool");
    }

    #[test]
    fn test_type_name_any() {
        assert_eq!(ArrayData::Any(vec![]).type_name(), "Any");
    }

    // ── sum_as_f64 ────────────────────────────────────────────────────────────

    #[test]
    fn test_sum_as_f64_integers() {
        let result = ArrayData::I64(vec![1, 2, 3]).sum_as_f64();
        assert!(
            (result - 6.0).abs() < 1e-15,
            "sum should be 6.0, got {}",
            result
        );
    }

    #[test]
    fn test_sum_as_f64_floats() {
        let result = ArrayData::F64(vec![1.5, 2.5]).sum_as_f64();
        assert!((result - 4.0).abs() < 1e-15);
    }

    #[test]
    fn test_sum_as_f64_booleans_count_true() {
        // true=1.0, false=0.0
        let result = ArrayData::Bool(vec![true, false, true, true]).sum_as_f64();
        assert!((result - 3.0).abs() < 1e-15);
    }

    #[test]
    fn test_sum_as_f64_bitpacked_booleans_count_true() {
        let result =
            ArrayData::BitPackedBool(BitPackedBoolData::from_bools(&[true, false, true, true]))
                .sum_as_f64();
        assert!((result - 3.0).abs() < 1e-15);
    }

    #[test]
    fn test_sum_as_f64_empty_is_zero() {
        assert_eq!(ArrayData::F64(vec![]).sum_as_f64(), 0.0);
    }

    #[test]
    fn test_sum_as_f64_string_is_zero() {
        // Non-numeric types contribute 0
        let result = ArrayData::String(vec![Value::str_new("a")]).sum_as_f64();
        assert_eq!(result, 0.0);
    }

    // ── get_value ─────────────────────────────────────────────────────────────

    #[test]
    fn test_get_value_f64_valid() {
        let data = ArrayData::F64(vec![1.25, 6.78]);
        assert!(matches!(data.get_value(0), Some(Value::F64(x)) if (x - 1.25).abs() < 1e-10));
        assert!(matches!(data.get_value(1), Some(Value::F64(x)) if (x - 6.78).abs() < 1e-10));
    }

    #[test]
    fn test_get_value_i64_valid() {
        let data = ArrayData::I64(vec![42, -7]);
        assert!(matches!(data.get_value(0), Some(Value::I64(42))));
        assert!(matches!(data.get_value(1), Some(Value::I64(-7))));
    }

    #[test]
    fn test_get_value_out_of_bounds_returns_none() {
        let data = ArrayData::I64(vec![1, 2]);
        assert!(
            data.get_value(10).is_none(),
            "out-of-bounds should return None"
        );
    }

    #[test]
    fn test_bitpacked_bool_get_set_push_pop() {
        let mut data = BitPackedBoolData::from_bools(&[true, false, true]);
        assert_eq!(data.raw_word_len(), 1);
        assert_eq!(data.get(1), Some(false));

        data.set(1, true).unwrap();
        data.push(false);
        assert_eq!(data.get(1), Some(true));
        assert_eq!(data.pop(), Some(false));
        assert_eq!(data.len(), 3);
    }

    // ── as_f64_slice ──────────────────────────────────────────────────────────

    #[test]
    fn test_as_f64_slice_for_f64_data() {
        let data = ArrayData::F64(vec![1.0, 2.0, 3.0]);
        let slice = data.as_f64_slice().unwrap();
        assert_eq!(slice, &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_as_f64_slice_for_non_f64_returns_none() {
        let data = ArrayData::I64(vec![1, 2]);
        assert!(data.as_f64_slice().is_none());
    }

    // ── StructF64 contiguous storage (Issue #9198 S4) ─────────────────────────

    #[test]
    fn test_structf64_element_type_and_metadata() {
        // The raw storage self-reports as F64 scalars; the logical isbits-struct
        // eltype is carried by the `StructInlineF64` override, not here.
        let data = ArrayData::StructF64(vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(data.element_type(), ArrayElementType::F64);
        assert_eq!(data.raw_len(), 4);
        assert!(!data.is_empty());
        assert_eq!(data.type_name(), "StructF64");
        // Not a numeric aggregate: `sum` dispatches to Julia `+`, never here.
        assert_eq!(data.sum_as_f64(), 0.0);
        // Not exposed as a flat f64 slice (it is interleaved AoS).
        assert!(data.as_f64_slice().is_none());
    }

    #[test]
    fn test_structf64_get_set_push_pop_raw_slots() {
        let mut data = ArrayData::StructF64(vec![1.0, 2.0]);
        assert!(matches!(data.get_value(0), Some(Value::F64(x)) if x == 1.0));
        assert!(matches!(data.get_value(1), Some(Value::F64(x)) if x == 2.0));
        data.set_value(1, Value::F64(9.0)).unwrap();
        assert!(matches!(data.get_value(1), Some(Value::F64(x)) if x == 9.0));
        data.push_value(Value::F64(3.0)).unwrap();
        assert_eq!(data.raw_len(), 3);
        assert!(matches!(data.pop_value(), Ok(Value::F64(x)) if x == 3.0));
        // Non-real values are rejected at the raw-slot level.
        assert!(data.set_value(0, Value::str_new("x")).is_err());
    }
}
