//! Iteration operations for values.

// SAFETY: i64→usize casts for range/array/struct iteration are from `r.length()` (≥ 0)
// or from iteration state indices that are non-negative by construction.
#![allow(clippy::cast_sign_loss)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use half::f16;

use crate::rng::RngLike;
use crate::types::JuliaType;
use crate::vm::error::VmError;
use crate::vm::exec::array_basic::array_element_type_from_julia_type;
use crate::vm::field_indices::{
    ARRAY_FIRST_DIM_INDEX, ARRAY_SECOND_DIM_INDEX, FIRST_FIELD_INDEX, FOURTH_FIELD_INDEX,
    SECOND_FIELD_INDEX, THIRD_FIELD_INDEX,
};
use crate::vm::hof_exec::state::HofOpKind;
use crate::vm::splat::{try_expand_splat_value_with_heap, KwargsMap, SplatPreparation};
use crate::vm::stack_ops::StackOps;
use crate::vm::value::is_native_array_value;
use crate::vm::value::{
    array_wrapper_value_from_array_value, array_wrapper_value_to_array_value,
    native_array_value_from_array, native_array_value_ref, ArrayElementType, ArrayValue,
    FunctionValue, GeneratorCallable, GeneratorValue, NamedTupleValue, PairsValue,
    RangeElementType, RangeValue, StructInstance, SymbolValue, TupleValue, Value,
};
use crate::vm::{TransientRootId, Vm};

fn range_uses_index_state(range: &RangeValue) -> bool {
    // Upstream `StepRangeLen`/`LinRange` iterate with an integer position;
    // OrdinalRange iterates with the current element value itself (#11387).
    range.is_explicit_float_type() || range.linspace_len.is_some()
}

fn first_range_iteration(range: &RangeValue) -> Result<Option<(Value, Value)>, VmError> {
    if range.length() <= 0 {
        return Ok(None);
    }
    let value = range
        .first_value()
        .ok_or_else(|| VmError::TypeError("iterate: range is empty".to_string()))?;
    let state = if range_uses_index_state(range) {
        Value::I64(1)
    } else {
        value.clone()
    };
    Ok(Some((value, state)))
}

fn integer_state_as_bigint(value: &Value) -> Option<num_bigint::BigInt> {
    match value {
        Value::I8(value) => Some(num_bigint::BigInt::from(*value)),
        Value::I16(value) => Some(num_bigint::BigInt::from(*value)),
        Value::I32(value) => Some(num_bigint::BigInt::from(*value)),
        Value::I64(value) => Some(num_bigint::BigInt::from(*value)),
        Value::I128(value) => Some(num_bigint::BigInt::from(*value)),
        Value::U8(value) => Some(num_bigint::BigInt::from(*value)),
        Value::U16(value) => Some(num_bigint::BigInt::from(*value)),
        Value::U32(value) => Some(num_bigint::BigInt::from(*value)),
        Value::U64(value) => Some(num_bigint::BigInt::from(*value)),
        Value::U128(value) => Some(num_bigint::BigInt::from(*value)),
        Value::BigInt(value) => Some(value.as_inner().clone()),
        _ => None,
    }
}

fn ordinal_state_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::I8(value) => Some(f64::from(*value)),
        Value::I16(value) => Some(f64::from(*value)),
        Value::I32(value) => Some(f64::from(*value)),
        Value::I64(value) => Some(*value as f64),
        Value::I128(value) => Some(*value as f64),
        Value::U8(value) => Some(f64::from(*value)),
        Value::U16(value) => Some(f64::from(*value)),
        Value::U32(value) => Some(f64::from(*value)),
        Value::U64(value) => Some(*value as f64),
        Value::U128(value) => Some(*value as f64),
        Value::F16(value) => Some(f64::from(*value)),
        Value::F32(value) => Some(f64::from(*value)),
        Value::F64(value) => Some(*value),
        Value::Char(value) => Some(f64::from(u32::from(*value))),
        _ => None,
    }
}

fn next_range_iteration(
    range: &RangeValue,
    state: &Value,
) -> Result<Option<(Value, Value)>, VmError> {
    let length = range.length();
    if length <= 0 {
        return Ok(None);
    }

    if range_uses_index_state(range) {
        let Value::I64(index) = state else {
            return Err(VmError::TypeError(
                "iterate: StepRangeLen state must be I64".to_string(),
            ));
        };
        if *index >= length {
            return Ok(None);
        }
        let next_index = index.saturating_add(1);
        let value = range.get_value(next_index)?;
        return Ok(Some((value, Value::I64(next_index))));
    }

    if let Some(parts) = &range.bigint {
        let current = integer_state_as_bigint(state).ok_or_else(|| {
            VmError::TypeError("iterate: ordinal range state must be an integer".to_string())
        })?;
        let last =
            parts.start.as_inner() + parts.step.as_inner() * num_bigint::BigInt::from(length - 1);
        if current == last {
            return Ok(None);
        }
        let next = current + parts.step.as_inner();
        let value = Value::BigInt(crate::vm::value::RustBigInt::from(next));
        return Ok(Some((value.clone(), value)));
    }

    let current = ordinal_state_as_f64(state).ok_or_else(|| {
        VmError::TypeError("iterate: ordinal range state must be numeric".to_string())
    })?;
    if current == range.get(length)? {
        return Ok(None);
    }
    let value = range.typed_element(current + range.step);
    Ok(Some((value.clone(), value)))
}

enum SplatValueCursor {
    Generic {
        collection: TransientRootId,
        state: Option<TransientRootId>,
    },
}

#[derive(Default)]
struct SplatKeywordScratch {
    entry: Option<TransientRootId>,
    key: Option<TransientRootId>,
    state: Option<TransientRootId>,
}

/// Shared string-iterate step (Issue #8995): decode the character starting at
/// the 1-based codeunit index `i`, returning `(char, next-state)` with
/// upstream's byte-offset state semantics (the state is the 1-based codeunit
/// index of the NEXT character), or `None` when `i` is out of bounds
/// (iteration finished). Malformed sequences yield their exact malformed
/// `Char`, and each step decodes in place — no per-step char collection.
fn string_iterate_at(bytes: &[u8], i: i64) -> Option<(Value, Value)> {
    if i < 1 || (i as u64) > bytes.len() as u64 {
        return None;
    }
    let (bits, next) = subset_julia_vm_bytecode::value::decode_julia_char(bytes, (i - 1) as usize);
    Some((Value::char_from_bits(bits), Value::I64(next as i64 + 1)))
}

impl<R: RngLike> Vm<R> {
    fn array_value(array: ArrayValue) -> Value {
        native_array_value_from_array(array)
    }

    fn is_zip_struct_name(name: &str) -> bool {
        matches!(name, "Zip" | "Zip3" | "Zip4" | "Zip5" | "Zip6" | "Zip7")
            || name.starts_with("Zip{")
            || name.starts_with("Zip3{")
            || name.starts_with("Zip4{")
            || name.starts_with("Zip5{")
            || name.starts_with("Zip6{")
            || name.starts_with("Zip7{")
    }

    fn is_enumerate_struct_name(name: &str) -> bool {
        name == "Enumerate" || name.starts_with("Enumerate{")
    }

    fn is_count_struct_name(name: &str) -> bool {
        name == "Count" || name.starts_with("Count{")
    }

    fn range_struct_base_name(name: &str) -> &str {
        name.rsplit('.')
            .next()
            .unwrap_or(name)
            .split('{')
            .next()
            .unwrap_or(name)
    }

    fn range_struct_element_type(name: &str) -> RangeElementType {
        let params = name
            .split_once('{')
            .map(|(_, rest)| rest)
            .unwrap_or_default();
        if params.contains("Char") {
            RangeElementType::Char
        } else if params.contains("UInt8") {
            RangeElementType::UInt8
        } else if params.contains("UInt16") {
            RangeElementType::UInt16
        } else if params.contains("UInt32") {
            RangeElementType::UInt32
        } else if params.contains("UInt64") {
            RangeElementType::UInt64
        } else if params.contains("Int8") {
            RangeElementType::Int8
        } else if params.contains("Int16") {
            RangeElementType::Int16
        } else if params.contains("Int32") {
            RangeElementType::Int32
        } else {
            RangeElementType::Default
        }
    }

    fn range_struct_field_to_f64(value: &Value) -> Option<f64> {
        match value {
            Value::Char(c) => Some(f64::from(u32::from(*c))),
            Value::I8(v) => Some(f64::from(*v)),
            Value::I16(v) => Some(f64::from(*v)),
            Value::I32(v) => Some(f64::from(*v)),
            Value::I64(v) => Some(*v as f64),
            Value::U8(v) => Some(f64::from(*v)),
            Value::U16(v) => Some(f64::from(*v)),
            Value::U32(v) => Some(f64::from(*v)),
            Value::U64(v) => Some(*v as f64),
            _ => None,
        }
    }

    fn range_struct_as_range_value(s: &StructInstance) -> Option<RangeValue> {
        let base = Self::range_struct_base_name(&s.struct_name);
        let element_type = Self::range_struct_element_type(&s.struct_name);
        match base {
            "UnitRange" => Some(RangeValue {
                start: Self::range_struct_field_to_f64(s.values.first()?)?,
                step: 1.0,
                stop: Self::range_struct_field_to_f64(s.values.get(1)?)?,
                is_float: false,
                element_type,
                step_type: RangeElementType::Default,
                is_step_range: false,
                linspace_len: None,
                step_defined: false,
                bigint: None,
            }),
            "StepRange" => Some(RangeValue {
                start: Self::range_struct_field_to_f64(s.values.first()?)?,
                step: Self::range_struct_field_to_f64(s.values.get(1)?)?,
                stop: Self::range_struct_field_to_f64(s.values.get(2)?)?,
                is_float: false,
                element_type,
                step_type: Self::range_struct_element_type(
                    s.struct_name
                        .split_once(',')
                        .map(|(_, step)| step.trim_end_matches('}').trim())
                        .unwrap_or_default(),
                ),
                is_step_range: true,
                linspace_len: None,
                step_defined: false,
                bigint: None,
            }),
            _ => None,
        }
    }

    /// Return `(nrows, ncols)` for matrix-like Array inputs that have at least
    /// two dimensions. Returns `None` when the input is a 1D Array (callers
    /// decide whether to treat the whole array as a single slice or to delegate
    /// to vector iteration). Errors for non-Array inputs.
    ///
    /// Centralized matrix-shape probe lets EachCol/EachRow/EachSlice iteration
    /// share a single native-Array match site instead of repeating raw
    /// `arr.borrow().shape` reads at each call site. The Array wrapper migration
    /// (Issue #3908) prefers routing matrix metadata through ArrayValue logical
    /// helpers as the runtime moves toward Memory{T} + Pure Julia Array.
    fn matrix_array_dims_2d(
        matrix: &Value,
        label: &str,
    ) -> Result<Option<(usize, usize)>, VmError> {
        let Some(arr) = native_array_value_ref(matrix) else {
            return Err(VmError::TypeError(format!(
                "{label}: matrix must be an Array"
            )));
        };
        let arr_borrow = arr.borrow();
        if arr_borrow.ndims() < 2 {
            return Ok(None);
        }
        let nrows = arr_borrow.shape[ARRAY_FIRST_DIM_INDEX];
        let ncols = arr_borrow.shape[ARRAY_SECOND_DIM_INDEX];
        Ok(Some((nrows, ncols)))
    }

    /// Extract row `row_1based` (1-indexed) from a 2D+ matrix Array and return
    /// it as a fresh 1D Array. Routes through `ArrayValue::any_vector` so the
    /// wrapping policy stays inside the value layer. Caller must ensure the
    /// matrix has at least 2 dimensions (use `matrix_array_dims_2d`).
    fn extract_matrix_row_1based(
        matrix: &Value,
        row_1based: i64,
        ncols: usize,
        label: &str,
    ) -> Result<Value, VmError> {
        let Some(arr) = native_array_value_ref(matrix) else {
            return Err(VmError::TypeError(format!(
                "{label}: matrix must be an Array"
            )));
        };
        let arr_borrow = arr.borrow();
        let mut row_data = Vec::with_capacity(ncols);
        for col in 0..ncols {
            let indices = vec![row_1based, col as i64 + 1];
            row_data.push(arr_borrow.get(&indices)?);
        }
        Ok(Self::array_value(ArrayValue::any_vector(row_data)))
    }

    /// Extract column `col_1based` (1-indexed) from a 2D+ matrix Array and
    /// return it as a fresh 1D Array. Caller must ensure the matrix has at
    /// least 2 dimensions.
    fn extract_matrix_column_1based(
        matrix: &Value,
        col_1based: i64,
        nrows: usize,
        label: &str,
    ) -> Result<Value, VmError> {
        let Some(arr) = native_array_value_ref(matrix) else {
            return Err(VmError::TypeError(format!(
                "{label}: matrix must be an Array"
            )));
        };
        let arr_borrow = arr.borrow();
        let mut column_data = Vec::with_capacity(nrows);
        for row in 0..nrows {
            let indices = vec![row as i64 + 1, col_1based];
            column_data.push(arr_borrow.get(&indices)?);
        }
        Ok(Self::array_value(ArrayValue::any_vector(column_data)))
    }

    fn array_wrapper_dims_and_offset(size: &Value) -> Result<(Vec<usize>, usize), VmError> {
        let Value::Tuple(size_tuple) = size else {
            return Err(VmError::TypeError(
                "Array: _size field must be a Tuple".to_string(),
            ));
        };

        if let Some(Value::Tuple(dims_tuple)) = size_tuple.elements.first() {
            let dims = Self::array_wrapper_dims_from_tuple(dims_tuple)?;
            let offset = match size_tuple.elements.get(1) {
                Some(Value::I64(i)) if *i >= 1 => *i as usize,
                Some(other) => {
                    return Err(VmError::TypeError(format!(
                        "Array: offset must be a positive Int64, got {:?}",
                        other.value_type()
                    )))
                }
                None => {
                    return Err(VmError::TypeError(
                        "Array: offset-encoded _size missing offset".to_string(),
                    ))
                }
            };
            return Ok((dims, offset));
        }

        Ok((Self::array_wrapper_dims_from_tuple(size_tuple)?, 1))
    }

    fn array_wrapper_dims_from_tuple(dims_tuple: &TupleValue) -> Result<Vec<usize>, VmError> {
        dims_tuple
            .elements
            .iter()
            .map(|dim| match dim {
                Value::I64(i) if *i >= 0 => Ok(*i as usize),
                other => Err(VmError::TypeError(format!(
                    "Array: dimensions must be non-negative Int64 values, got {:?}",
                    other.value_type()
                ))),
            })
            .collect()
    }

    fn iterate_array_wrapper_fields(
        &self,
        fields: &[Value],
        linear: usize,
    ) -> Result<Value, VmError> {
        let Some(mem) = fields.first() else {
            return Err(VmError::TypeError("Array: missing _mem field".to_string()));
        };
        let Some(size) = fields.get(1) else {
            return Err(VmError::TypeError("Array: missing _size field".to_string()));
        };
        let (dims, offset) = Self::array_wrapper_dims_and_offset(size)?;
        let len: usize = dims.iter().product();
        if linear >= len {
            return Ok(Value::Nothing);
        }

        let elem = match mem {
            Value::MemoryRef(memref) => memref.get(linear + 1)?,
            Value::Memory(memory) => memory.borrow().get(offset + linear)?,
            _ if is_native_array_value(mem) => {
                let array = native_array_value_ref(mem).ok_or_else(|| {
                    VmError::TypeError(
                        "Array: _mem field unexpectedly lost native Array storage".to_string(),
                    )
                })?;
                array.borrow().get_linear(offset - 1 + linear)?
            }
            other => {
                return Err(VmError::TypeError(format!(
                    "Array: _mem field must be Memory or Array, got {:?}",
                    other.value_type()
                )))
            }
        };
        let next_state = Value::I64((linear + 2) as i64);
        Ok(Value::Tuple(TupleValue {
            elements: vec![elem, next_state],
        }))
    }

    fn collect_array_wrapper_fields(&mut self, fields: &[Value]) -> Result<Value, VmError> {
        let Some(mem) = fields.first() else {
            return Err(VmError::TypeError("Array: missing _mem field".to_string()));
        };
        let Some(size) = fields.get(1) else {
            return Err(VmError::TypeError("Array: missing _size field".to_string()));
        };
        let (dims, offset) = Self::array_wrapper_dims_and_offset(size)?;
        let len: usize = dims.iter().product();

        let (values, element_type) = match mem {
            Value::MemoryRef(memref) => {
                let mut values = Vec::with_capacity(len);
                for linear in 0..len {
                    values.push(memref.get(linear + 1)?);
                }
                (values, memref.element_type())
            }
            Value::Memory(memory) => {
                let borrowed = memory.borrow();
                let mut values = Vec::with_capacity(len);
                for linear in 0..len {
                    values.push(borrowed.get(offset + linear)?);
                }
                (values, borrowed.element_type().clone())
            }
            _ if is_native_array_value(mem) => {
                let array = native_array_value_ref(mem).ok_or_else(|| {
                    VmError::TypeError(
                        "Array: _mem field unexpectedly lost native Array storage".to_string(),
                    )
                })?;
                let borrowed = array.borrow();
                let mut values = Vec::with_capacity(len);
                for linear in 0..len {
                    values.push(borrowed.get_linear(offset - 1 + linear)?);
                }
                (values, borrowed.element_type())
            }
            other => {
                return Err(VmError::TypeError(format!(
                    "Array: _mem field must be Memory or Array, got {:?}",
                    other.value_type()
                )))
            }
        };

        // CollectFallback: array-wrapper-copy-materialization (Issue #4579).
        let mut arr = ArrayValue::memory_first_collect_typejoin_values(values, element_type)?;
        arr.shape = dims;
        self.array_wrapper_value(arr)
    }

    fn iterate_first_count_fields(&self, fields: &[Value]) -> Result<Value, VmError> {
        let (Some(start), Some(step)) = (fields.first(), fields.get(1)) else {
            return Err(VmError::TypeError(
                "countfrom: missing start or step".to_string(),
            ));
        };
        let next_state = Self::count_next_state(start, step)?;
        Ok(Value::Tuple(TupleValue {
            elements: vec![start.clone(), next_state],
        }))
    }

    fn iterate_next_count_fields(&self, fields: &[Value], state: &Value) -> Result<Value, VmError> {
        let Some(step) = fields.get(1) else {
            return Err(VmError::TypeError("countfrom: missing step".to_string()));
        };
        let next_state = Self::count_next_state(state, step)?;
        Ok(Value::Tuple(TupleValue {
            elements: vec![state.clone(), next_state],
        }))
    }

    fn count_next_state(state: &Value, step: &Value) -> Result<Value, VmError> {
        match (state, step) {
            (Value::I64(x), Value::I64(s)) => Ok(Value::I64(x + s)),
            (Value::F64(x), Value::F64(s)) => Ok(Value::F64(x + s)),
            (Value::I64(x), Value::F64(s)) => Ok(Value::F64(*x as f64 + s)),
            (Value::F64(x), Value::I64(s)) => Ok(Value::F64(x + *s as f64)),
            _ => Err(VmError::TypeError(
                "countfrom: state and step must be numeric".to_string(),
            )),
        }
    }

    fn iterate_first_enumerate_fields(&self, fields: &[Value]) -> Result<Value, VmError> {
        let Some(iter) = fields.first() else {
            return Err(VmError::TypeError(
                "enumerate: missing wrapped iterator".to_string(),
            ));
        };
        match self.iterate_first(iter)? {
            Value::Tuple(tuple) if tuple.elements.len() == 2 => Ok(Value::Tuple(TupleValue {
                elements: vec![
                    Value::Tuple(TupleValue {
                        elements: vec![Value::I64(1), tuple.elements[0].clone()],
                    }),
                    Value::Tuple(TupleValue {
                        elements: vec![Value::I64(2), tuple.elements[1].clone()],
                    }),
                ],
            })),
            Value::Nothing => Ok(Value::Nothing),
            _ => Err(VmError::TypeError(
                "enumerate: iterate result must be Tuple or Nothing".to_string(),
            )),
        }
    }

    fn iterate_next_enumerate_fields(
        &self,
        fields: &[Value],
        state: &Value,
    ) -> Result<Value, VmError> {
        let Some(iter) = fields.first() else {
            return Err(VmError::TypeError(
                "enumerate: missing wrapped iterator".to_string(),
            ));
        };
        let Value::Tuple(state_tuple) = state else {
            return Err(VmError::TypeError(
                "enumerate: state must be a tuple".to_string(),
            ));
        };
        let (Some(Value::I64(i)), Some(inner_state)) =
            (state_tuple.elements.first(), state_tuple.elements.get(1))
        else {
            return Err(VmError::TypeError(
                "enumerate: state must be (counter, inner_state)".to_string(),
            ));
        };
        match self.iterate_next(iter, inner_state)? {
            Value::Tuple(tuple) if tuple.elements.len() == 2 => Ok(Value::Tuple(TupleValue {
                elements: vec![
                    Value::Tuple(TupleValue {
                        elements: vec![Value::I64(*i), tuple.elements[0].clone()],
                    }),
                    Value::Tuple(TupleValue {
                        elements: vec![Value::I64(*i + 1), tuple.elements[1].clone()],
                    }),
                ],
            })),
            Value::Nothing => Ok(Value::Nothing),
            _ => Err(VmError::TypeError(
                "enumerate: iterate result must be Tuple or Nothing".to_string(),
            )),
        }
    }

    fn iterate_first_zip_fields(&self, fields: &[Value]) -> Result<Value, VmError> {
        let mut values = Vec::with_capacity(fields.len());
        let mut states = Vec::with_capacity(fields.len());
        for field in fields {
            match self.iterate_first(field)? {
                Value::Tuple(tuple) if tuple.elements.len() == 2 => {
                    values.push(tuple.elements[0].clone());
                    states.push(tuple.elements[1].clone());
                }
                Value::Nothing => return Ok(Value::Nothing),
                other => {
                    return Err(VmError::TypeError(format!(
                        "zip: expected iterate result tuple, got {:?}",
                        other.runtime_type()
                    )));
                }
            }
        }
        Ok(Value::Tuple(TupleValue {
            elements: vec![
                Value::Tuple(TupleValue { elements: values }),
                Value::Tuple(TupleValue { elements: states }),
            ],
        }))
    }

    fn iterate_next_zip_fields(&self, fields: &[Value], state: &Value) -> Result<Value, VmError> {
        let Value::Tuple(state_tuple) = state else {
            return Err(VmError::TypeError(
                "zip: state must be a tuple of iterator states".to_string(),
            ));
        };
        if state_tuple.elements.len() != fields.len() {
            return Err(VmError::TypeError(format!(
                "zip: expected {} state entries, got {}",
                fields.len(),
                state_tuple.elements.len()
            )));
        }

        let mut values = Vec::with_capacity(fields.len());
        let mut states = Vec::with_capacity(fields.len());
        for (field, field_state) in fields.iter().zip(state_tuple.elements.iter()) {
            match self.iterate_next(field, field_state)? {
                Value::Tuple(tuple) if tuple.elements.len() == 2 => {
                    values.push(tuple.elements[0].clone());
                    states.push(tuple.elements[1].clone());
                }
                Value::Nothing => return Ok(Value::Nothing),
                other => {
                    return Err(VmError::TypeError(format!(
                        "zip: expected iterate result tuple, got {:?}",
                        other.runtime_type()
                    )));
                }
            }
        }
        Ok(Value::Tuple(TupleValue {
            elements: vec![
                Value::Tuple(TupleValue { elements: values }),
                Value::Tuple(TupleValue { elements: states }),
            ],
        }))
    }

    fn apply_generator_callable_to_value(
        &self,
        callable: &GeneratorCallable,
        value: &Value,
    ) -> Result<Value, VmError> {
        match callable {
            GeneratorCallable::TypeObject(jt) => {
                crate::vm::convert::convert_value(&jt.name(), value)
            }
            GeneratorCallable::TupleSplatTypeObject(jt) => match value {
                Value::Tuple(tuple) => self.construct_type_object_from_args(jt, &tuple.elements),
                _ => Err(VmError::MethodError(format!(
                    "no method matching {}({})",
                    jt.name(),
                    self.get_type_name(value)
                ))),
            },
            _ => Ok(value.clone()),
        }
    }

    fn construct_type_object_from_args(
        &self,
        julia_type: &crate::types::JuliaType,
        args: &[Value],
    ) -> Result<Value, VmError> {
        let type_name = julia_type.name();
        if args.len() == 1 {
            return crate::vm::convert::convert_value(&type_name, &args[0]);
        }

        let Some((type_id, field_count)) = self
            .struct_defs
            .iter()
            .enumerate()
            .find(|(_, def)| def.name == type_name)
            .or_else(|| {
                let base_name = type_name.split_once('{')?.0;
                self.struct_defs
                    .iter()
                    .enumerate()
                    .find(|(_, def)| def.name == base_name)
            })
            .map(|(type_id, def)| (type_id, def.fields.len()))
        else {
            return Err(VmError::MethodError(format!(
                "no method matching {}({})",
                type_name,
                args.iter()
                    .map(|arg| self.get_type_name(arg))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        };

        if field_count != args.len() {
            return Err(VmError::MethodError(format!(
                "no method matching {}({})",
                type_name,
                args.iter()
                    .map(|arg| self.get_type_name(arg))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }

        Ok(Value::Struct(StructInstance::with_name(
            type_id,
            type_name.into_owned(),
            args.to_vec(),
        )))
    }

    fn apply_generator_callable_to_iterate_result(
        &self,
        callable: &GeneratorCallable,
        next: Value,
    ) -> Result<Value, VmError> {
        match next {
            Value::Tuple(mut tuple) if tuple.elements.len() == 2 => {
                let converted =
                    self.apply_generator_callable_to_value(callable, &tuple.elements[0])?;
                tuple.elements[0] = converted;
                Ok(Value::Tuple(tuple))
            }
            other => Ok(other),
        }
    }

    /// Get the type_id for CartesianIndex from struct_defs (cached).
    /// Falls back to 0 if not found (for backwards compatibility).
    fn get_cartesian_index_type_id(&self) -> usize {
        if let Some(id) = self.cached_cartesian_index_type_id.get() {
            return id;
        }
        let id = self
            .struct_defs
            .iter()
            .position(|def| def.name == "CartesianIndex")
            .unwrap_or(0);
        self.cached_cartesian_index_type_id.set(Some(id));
        id
    }

    /// Get the type_id for Pair from struct_defs (cached).
    /// Falls back to 0 if not found (for backwards compatibility).
    fn get_pair_type_id(&self) -> usize {
        if let Some(id) = self.cached_pair_type_id.get() {
            return id;
        }
        let id = self
            .struct_defs
            .iter()
            .position(|def| def.name == "Pair")
            .unwrap_or(0);
        self.cached_pair_type_id.set(Some(id));
        id
    }

    pub(crate) fn get_array_type_id(&self) -> usize {
        if let Some(id) = self.cached_array_type_id.get() {
            if self
                .struct_defs
                .get(id)
                .is_some_and(|def| Self::is_array_struct_def_name(&def.name))
            {
                return id;
            }
        }
        let Some(id) = self
            .struct_defs
            .iter()
            .position(|def| Self::is_array_struct_def_name(&def.name))
        else {
            return 0;
        };
        self.cached_array_type_id.set(Some(id));
        id
    }

    fn is_array_struct_def_name(name: &str) -> bool {
        name == "Array" || name == "Base.Array" || name.starts_with("Array{")
    }

    fn is_array_wrapper_struct_name(name: &str) -> bool {
        name == "Array" || name.starts_with("Array{")
    }

    pub(crate) fn array_wrapper_value(&mut self, array: ArrayValue) -> Result<Value, VmError> {
        array_wrapper_value_from_array_value(array, self.get_array_type_id(), &mut self.struct_heap)
    }

    pub(crate) fn pairs_value_element_type_name(&self, values: &[Value]) -> String {
        let Some((first, rest)) = values.split_first() else {
            return "Union{}".to_string();
        };
        let mut element_type = Self::pairs_value_array_element_type(first);
        for value in rest {
            element_type = Self::typejoin_pairs_value_types(
                element_type,
                Self::pairs_value_array_element_type(value),
            );
        }
        element_type.julia_type_name()
    }

    pub(crate) fn pairs_runtime_type_name(&self, pairs: &PairsValue) -> String {
        let value_type_name = self.pairs_value_element_type_name(&pairs.data.values);
        let fields = pairs
            .data
            .names
            .iter()
            .zip(pairs.data.values.iter())
            .map(|(name, value)| format!("{}::{}", name, self.get_type_name(value)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("Base.Pairs{{Symbol, {value_type_name}, Nothing, @NamedTuple{{{fields}}}}}")
    }

    fn pairs_value_array_element_type(value: &Value) -> ArrayElementType {
        match value {
            Value::I8(_) => ArrayElementType::I8,
            Value::I16(_) => ArrayElementType::I16,
            Value::I32(_) => ArrayElementType::I32,
            Value::I64(_) => ArrayElementType::I64,
            Value::I128(_) => ArrayElementType::I128,
            Value::BigInt(_) => ArrayElementType::Abstract("BigInt".to_string()),
            Value::U8(_) => ArrayElementType::U8,
            Value::U16(_) => ArrayElementType::U16,
            Value::U32(_) => ArrayElementType::U32,
            Value::U64(_) => ArrayElementType::U64,
            Value::U128(_) => ArrayElementType::U128,
            // Issue #9301: Float16 pair values narrow like F32/F64 (boxed tag).
            Value::F16(_) => ArrayElementType::F16,
            Value::F32(_) => ArrayElementType::F32,
            Value::F64(_) => ArrayElementType::F64,
            Value::BigFloat(_) => ArrayElementType::Abstract("BigFloat".to_string()),
            Value::Bool(_) => ArrayElementType::Bool,
            Value::Str(_) | Value::StrBytes(_) => ArrayElementType::String,
            Value::Char(_) | Value::CharMalformed(_) => ArrayElementType::Char,
            Value::Symbol(_) => ArrayElementType::Symbol,
            _ => ArrayElementType::Any,
        }
    }

    fn typejoin_pairs_value_types(
        left: ArrayElementType,
        right: ArrayElementType,
    ) -> ArrayElementType {
        if left == right {
            return left;
        }
        if let Some(common) = Self::typejoin_pairs_numeric_abstract_name(&left, &right) {
            return ArrayElementType::Abstract(common.to_string());
        }
        ArrayElementType::Any
    }

    fn typejoin_pairs_numeric_abstract_name(
        left: &ArrayElementType,
        right: &ArrayElementType,
    ) -> Option<&'static str> {
        let left_chain = Self::pairs_numeric_abstract_chain(left)?;
        let right_chain = Self::pairs_numeric_abstract_chain(right)?;
        left_chain
            .iter()
            .find(|candidate| right_chain.contains(candidate))
            .copied()
    }

    fn pairs_numeric_abstract_chain(
        element_type: &ArrayElementType,
    ) -> Option<&'static [&'static str]> {
        match element_type {
            ArrayElementType::Bool => Some(&["Integer", "Real", "Number", "Any"]),
            ArrayElementType::I8
            | ArrayElementType::I16
            | ArrayElementType::I32
            | ArrayElementType::I64
            | ArrayElementType::I128 => Some(&["Signed", "Integer", "Real", "Number", "Any"]),
            ArrayElementType::U8
            | ArrayElementType::U16
            | ArrayElementType::U32
            | ArrayElementType::U64
            | ArrayElementType::U128 => Some(&["Unsigned", "Integer", "Real", "Number", "Any"]),
            ArrayElementType::F16 | ArrayElementType::F32 | ArrayElementType::F64 => {
                Some(&["AbstractFloat", "Real", "Number", "Any"])
            }
            ArrayElementType::Abstract(name) => match name.as_str() {
                "BigInt" => Some(&["BigInt", "Signed", "Integer", "Real", "Number", "Any"]),
                "BigFloat" => Some(&["BigFloat", "AbstractFloat", "Real", "Number", "Any"]),
                "Signed" => Some(&["Signed", "Integer", "Real", "Number", "Any"]),
                "Unsigned" => Some(&["Unsigned", "Integer", "Real", "Number", "Any"]),
                "Integer" => Some(&["Integer", "Real", "Number", "Any"]),
                "AbstractFloat" => Some(&["AbstractFloat", "Real", "Number", "Any"]),
                "Real" => Some(&["Real", "Number", "Any"]),
                "Number" => Some(&["Number", "Any"]),
                "Any" => Some(&["Any"]),
                _ => None,
            },
            _ => None,
        }
    }

    fn pairs_entry_value(&self, name: &str, value: &Value, _value_type_name: &str) -> Value {
        Value::Struct(StructInstance {
            type_id: self.get_pair_type_id(),
            struct_name: "Pair".into(),
            values: vec![Value::Symbol(SymbolValue::new(name)), value.clone()],
        })
    }

    /// Check if a value is missing.
    fn is_missing(&self, val: &Value) -> bool {
        matches!(val, Value::Missing)
    }

    /// Issue #5168: tuple-free fast path for `iterate(coll)` on builtin
    /// collections (Array/Memory/Range/Tuple/String). Returns:
    ///   - `Ok(None)`               → not a fast-path collection; the caller must
    ///                                fall back to the tuple-returning `iterate_first`.
    ///   - `Ok(Some(None))`         → fast-path collection, but empty/exhausted.
    ///   - `Ok(Some(Some((e, s))))` → fast-path collection yielding element `e`
    ///                                and next state `s`.
    ///
    /// This mirrors `iterate_first` exactly for the covered types but never
    /// allocates the `(element, state)` tuple, so the generic ForEach loop can
    /// push the two values directly onto the stack.
    pub(in crate::vm) fn iterate_first_fast(
        &self,
        coll: &Value,
    ) -> Result<Option<Option<(Value, Value)>>, VmError> {
        match coll {
            _ if is_native_array_value(coll) => {
                let arr = native_array_value_ref(coll).ok_or_else(|| {
                    VmError::TypeError(
                        "iterate: collection unexpectedly lost native Array storage".to_string(),
                    )
                })?;
                let arr_borrow = arr.borrow();
                if arr_borrow.element_count() == 0 {
                    Ok(Some(None))
                } else {
                    let elem = arr_borrow.get_linear(0)?;
                    Ok(Some(Some((elem, Value::I64(2)))))
                }
            }
            Value::Memory(mem) => {
                let mem_borrow = mem.borrow();
                let len = mem_borrow.len();
                if len == 0 {
                    Ok(Some(None))
                } else {
                    let elem = mem_borrow.get(1)?;
                    Ok(Some(Some((elem, Value::I64(2)))))
                }
            }
            Value::Range(range) => Ok(Some(first_range_iteration(range)?)),
            Value::Tuple(t) => {
                if t.elements.is_empty() {
                    Ok(Some(None))
                } else {
                    Ok(Some(Some((t.elements[0].clone(), Value::I64(2)))))
                }
            }
            // Strings iterate with upstream's byte-offset state: the state is
            // the 1-based codeunit index of the NEXT character, and malformed
            // sequences yield their exact malformed Char (Issue #8995).
            Value::Str(s) => Ok(Some(string_iterate_at(s.as_bytes(), 1))),
            Value::StrBytes(bytes) => Ok(Some(string_iterate_at(bytes.as_ref(), 1))),
            _ => Ok(None),
        }
    }

    /// Issue #5168: tuple-free fast path for `iterate(coll, state)` on builtin
    /// collections. See `iterate_first_fast` for the return-value contract.
    pub(in crate::vm) fn iterate_next_fast(
        &self,
        coll: &Value,
        state: &Value,
    ) -> Result<Option<Option<(Value, Value)>>, VmError> {
        if let Value::Range(range) = coll {
            return next_range_iteration(range, state).map(Some);
        }

        // Only handle the fast-path collections with an I64 state. Anything else
        // (including non-I64 states) defers to the generic tuple path.
        let idx = match (coll, state) {
            (Value::Tuple(_), Value::I64(i))
            | (Value::Str(_), Value::I64(i))
            | (Value::StrBytes(_), Value::I64(i))
            | (Value::Memory(_), Value::I64(i)) => *i as usize,
            (_, Value::I64(i)) if is_native_array_value(coll) => *i as usize,
            _ => return Ok(None),
        };

        match coll {
            _ if is_native_array_value(coll) => {
                let arr = native_array_value_ref(coll).ok_or_else(|| {
                    VmError::TypeError(
                        "iterate: collection unexpectedly lost native Array storage".to_string(),
                    )
                })?;
                let arr_borrow = arr.borrow();
                if idx == 0 || idx > arr_borrow.element_count() {
                    Ok(Some(None))
                } else {
                    let elem = arr_borrow.get_linear(idx - 1)?;
                    Ok(Some(Some((elem, Value::I64((idx + 1) as i64)))))
                }
            }
            Value::Memory(mem) => {
                let mem_borrow = mem.borrow();
                if idx == 0 || idx > mem_borrow.len() {
                    Ok(Some(None))
                } else {
                    let elem = mem_borrow.get(idx)?;
                    Ok(Some(Some((elem, Value::I64((idx + 1) as i64)))))
                }
            }
            Value::Tuple(t) => {
                if idx == 0 || idx > t.elements.len() {
                    Ok(Some(None))
                } else {
                    Ok(Some(Some((
                        t.elements[idx - 1].clone(),
                        Value::I64((idx + 1) as i64),
                    ))))
                }
            }
            // Byte-offset iterate state (Issue #8995): `idx` is the 1-based
            // codeunit index of the next character; decode in place (linear,
            // no per-step full-string char collection).
            Value::Str(s) => Ok(Some(string_iterate_at(s.as_bytes(), idx as i64))),
            Value::StrBytes(bytes) => Ok(Some(string_iterate_at(bytes.as_ref(), idx as i64))),
            _ => Ok(None),
        }
    }

    /// Dispatch `iterate(first)` for a heap `StructRef` by its struct name
    /// (ranges, `Each*`, `CartesianIndices`, `SkipMissing`, zip/enumerate/count,
    /// ...). Mirrors the original inline match: named arms early-return, any
    /// unsupported struct falls through to the trailing error. Extracted from
    /// `iterate_first` to keep it flat (Issue #6833).
    fn iterate_first_struct_dispatch(
        &self,
        s: &StructInstance,
        coll: &Value,
        idx: usize,
    ) -> Result<Value, VmError> {
        if Self::is_array_wrapper_struct_name(&s.struct_name) {
            return self.iterate_array_wrapper_fields(&s.values, 0);
        }
        match &*s.struct_name {
            name if Self::is_zip_struct_name(name) => {
                return self.iterate_first_zip_fields(&s.values);
            }
            name if Self::is_enumerate_struct_name(name) => {
                return self.iterate_first_enumerate_fields(&s.values);
            }
            name if Self::is_count_struct_name(name) => {
                return self.iterate_first_count_fields(&s.values);
            }
            name if matches!(
                Self::range_struct_base_name(name),
                "UnitRange" | "StepRange"
            ) =>
            {
                let range = Self::range_struct_as_range_value(s).ok_or_else(|| {
                    VmError::TypeError(format!("{}: invalid range struct fields", s.struct_name))
                })?;
                if range.length() <= 0 {
                    return Ok(Value::Nothing);
                }
                let elem = range
                    .first_value()
                    .ok_or_else(|| VmError::TypeError("iterate: range is empty".to_string()))?;
                return Ok(Value::Tuple(TupleValue {
                    elements: vec![elem, Value::I64(1)],
                }));
            }
            "CartesianIndices" => {
                let dims = s
                    .values
                    .first()
                    .cloned()
                    .unwrap_or(Value::Tuple(TupleValue { elements: vec![] }));
                if let Value::Tuple(dims_tuple) = dims {
                    if dims_tuple.elements.is_empty() {
                        let ci = StructInstance {
                            type_id: self.get_cartesian_index_type_id(),
                            struct_name: "CartesianIndex".into(),
                            values: vec![Value::Tuple(TupleValue { elements: vec![] })],
                        };
                        let state = Value::Bool(true);
                        return Ok(Value::Tuple(TupleValue {
                            elements: vec![Value::Struct(ci), state],
                        }));
                    } else {
                        for d in &dims_tuple.elements {
                            if let Value::I64(v) = d {
                                if *v <= 0 {
                                    return Ok(Value::Nothing);
                                }
                            }
                        }
                        let n = dims_tuple.elements.len();
                        let first_idx: Vec<Value> = (0..n).map(|_| Value::I64(1)).collect();
                        let ci = StructInstance {
                            type_id: self.get_cartesian_index_type_id(),
                            struct_name: "CartesianIndex".into(),
                            values: vec![Value::Tuple(TupleValue {
                                elements: first_idx.clone(),
                            })],
                        };
                        let state = Value::Tuple(TupleValue {
                            elements: first_idx,
                        });
                        return Ok(Value::Tuple(TupleValue {
                            elements: vec![Value::Struct(ci), state],
                        }));
                    }
                }
            }
            "OneTo" => {
                // OneTo iteration: simple range iteration
                if let Some(stop_val) = s.values.first() {
                    if let Value::I64(stop) = stop_val {
                        if *stop <= 0 {
                            return Ok(Value::Nothing);
                        }
                        // Return first element (1) and state (2)
                        return Ok(Value::Tuple(TupleValue {
                            elements: vec![Value::I64(1), Value::I64(2)],
                        }));
                    } else {
                        return Err(VmError::TypeError(
                            "OneTo: stop must be an integer".to_string(),
                        ));
                    }
                }
            }
            "EachCol" => {
                // EachCol iteration: iterate over columns of a matrix.
                // Route through matrix_array_dims_2d + extract_matrix_column_1based
                // so raw ArrayValue storage stays inside the value-layer helpers
                // (Issue #3908).
                if let Some(mat) = s.values.first() {
                    match Self::matrix_array_dims_2d(mat, "EachCol")? {
                        None => {
                            // 1D array: treat as single column
                            let col = mat.clone();
                            return Ok(Value::Tuple(TupleValue {
                                elements: vec![col, Value::I64(2)],
                            }));
                        }
                        Some((nrows, ncols)) => {
                            if ncols == 0 {
                                return Ok(Value::Nothing);
                            }
                            let col = Self::extract_matrix_column_1based(mat, 1, nrows, "EachCol")?;
                            return Ok(Value::Tuple(TupleValue {
                                elements: vec![col, Value::I64(2)],
                            }));
                        }
                    }
                }
            }
            "EachRow" => {
                // EachRow iteration: iterate over rows of a matrix.
                // Logical 2D probe + row-extraction helper keeps raw Array
                // storage matching inside ArrayValue helpers (Issue #3908).
                if let Some(mat) = s.values.first() {
                    match Self::matrix_array_dims_2d(mat, "EachRow")? {
                        None => {
                            // 1D array: each element is a "row"
                            return self.iterate_first(mat);
                        }
                        Some((nrows, ncols)) => {
                            if nrows == 0 {
                                return Ok(Value::Nothing);
                            }
                            let row = Self::extract_matrix_row_1based(mat, 1, ncols, "EachRow")?;
                            return Ok(Value::Tuple(TupleValue {
                                elements: vec![row, Value::I64(2)],
                            }));
                        }
                    }
                }
            }
            "EachSlice" => {
                // EachSlice iteration: iterate over slices along a specified
                // dimension. Logical dims probe + row/column helpers keep raw
                // ArrayValue storage matching centralized (Issue #3908).
                if let (Some(mat), Some(dim_val)) = (s.values.first(), s.values.get(1)) {
                    let dim = match dim_val {
                        Value::I64(d) => *d as usize,
                        _ => {
                            return Err(VmError::TypeError(
                                "EachSlice: dim must be an integer".to_string(),
                            ))
                        }
                    };
                    match Self::matrix_array_dims_2d(mat, "EachSlice")? {
                        None => {
                            if dim == 1 {
                                return self.iterate_first(mat);
                            } else {
                                let col = mat.clone();
                                return Ok(Value::Tuple(TupleValue {
                                    elements: vec![col, Value::I64(2)],
                                }));
                            }
                        }
                        Some((nrows, ncols)) => {
                            let n = if dim == 1 { nrows } else { ncols };
                            if n == 0 {
                                return Ok(Value::Nothing);
                            }
                            let slice = if dim == 1 {
                                Self::extract_matrix_row_1based(mat, 1, ncols, "EachSlice")?
                            } else {
                                Self::extract_matrix_column_1based(mat, 1, nrows, "EachSlice")?
                            };
                            return Ok(Value::Tuple(TupleValue {
                                elements: vec![slice, Value::I64(2)],
                            }));
                        }
                    }
                }
            }
            "SkipMissing" => {
                // SkipMissing iteration: iterate over the underlying collection, skipping missing values
                // The Julia-defined iterate methods handle the logic
                if let Some(inner_coll) = s.values.first() {
                    // Start by iterating the inner collection
                    let next = self.iterate_first(inner_coll)?;
                    if matches!(next, Value::Nothing) {
                        return Ok(Value::Nothing);
                    }
                    // Extract (val, state) from the result
                    if let Value::Tuple(t) = &next {
                        if t.elements.len() == 2 {
                            let val = &t.elements[0];
                            let state = &t.elements[1];
                            // Check if value is missing
                            if self.is_missing(val) {
                                // Skip this missing value and continue to next
                                return self.iterate_next(coll, state);
                            }
                            // Return the non-missing value with state
                            return Ok(next);
                        }
                    }
                    return Ok(next);
                }
            }
            name if name.starts_with("LinRange") && s.values.len() >= 3 => {
                // LinRange iteration: linearly spaced range
                // Fields: start, stop, len, lendiv
                let len = match &s.values[THIRD_FIELD_INDEX] {
                    Value::I64(n) => *n,
                    _ => return Err(VmError::TypeError("LinRange: len must be I64".to_string())),
                };
                if len <= 0 {
                    return Ok(Value::Nothing);
                }
                // Return first element using lerp formula: start (when i=1)
                let start = match &s.values[FIRST_FIELD_INDEX] {
                    Value::F64(f) => *f,
                    Value::I64(i) => *i as f64,
                    _ => {
                        return Err(VmError::TypeError(
                            "LinRange: start must be numeric".to_string(),
                        ))
                    }
                };
                return Ok(Value::Tuple(TupleValue {
                    elements: vec![Value::F64(start), Value::I64(1)],
                }));
            }
            name if name.starts_with("StepRangeLen") && s.values.len() >= 4 => {
                // StepRangeLen iteration: range with reference, step, length, offset
                // Fields: ref, step, len, offset
                let len = match &s.values[THIRD_FIELD_INDEX] {
                    Value::I64(n) => *n,
                    _ => {
                        return Err(VmError::TypeError(
                            "StepRangeLen: len must be I64".to_string(),
                        ))
                    }
                };
                if len <= 0 {
                    return Ok(Value::Nothing);
                }
                let ref_val = match &s.values[FIRST_FIELD_INDEX] {
                    Value::F64(f) => *f,
                    Value::I64(i) => *i as f64,
                    _ => {
                        return Err(VmError::TypeError(
                            "StepRangeLen: ref must be numeric".to_string(),
                        ))
                    }
                };
                let step_val = match &s.values[SECOND_FIELD_INDEX] {
                    Value::F64(f) => *f,
                    Value::I64(i) => *i as f64,
                    _ => {
                        return Err(VmError::TypeError(
                            "StepRangeLen: step must be numeric".to_string(),
                        ))
                    }
                };
                let offset = match &s.values[FOURTH_FIELD_INDEX] {
                    Value::I64(n) => *n,
                    _ => {
                        return Err(VmError::TypeError(
                            "StepRangeLen: offset must be I64".to_string(),
                        ))
                    }
                };
                // First element: ref + (1 - offset) * step
                let first_val = ref_val + (1.0 - offset as f64) * step_val;
                return Ok(Value::Tuple(TupleValue {
                    elements: vec![Value::F64(first_val), Value::I64(1)],
                }));
            }
            name if name.starts_with("LogRange") && s.values.len() >= 5 => {
                // LogRange iteration: logarithmically spaced values.
                // Fields: start, stop, len, log_start_div, log_stop_div.
                let len = match &s.values[THIRD_FIELD_INDEX] {
                    Value::I64(n) => *n,
                    _ => return Err(VmError::TypeError("LogRange: len must be I64".to_string())),
                };
                if len <= 0 {
                    return Ok(Value::Nothing);
                }
                let start = match &s.values[FIRST_FIELD_INDEX] {
                    Value::F64(f) => *f,
                    Value::I64(i) => *i as f64,
                    _ => {
                        return Err(VmError::TypeError(
                            "LogRange: start must be numeric".to_string(),
                        ))
                    }
                };
                return Ok(Value::Tuple(TupleValue {
                    elements: vec![Value::F64(start), Value::I64(1)],
                }));
            }
            _ => {
                // Other struct types are unsupported
            }
        }
        Err(VmError::TypeError(format!(
            "iterate: unsupported struct type for StructRef({})",
            idx
        )))
    }

    pub(in crate::vm) fn iterate_first(&self, coll: &Value) -> Result<Value, VmError> {
        // Core.SimpleVector iterates exactly like a Tuple (Issue #4722).
        if let Value::SimpleVector(sv) = coll {
            return self.iterate_first(&Value::Tuple(sv.clone()));
        }
        // A NamedTuple iterates over its field values in declaration order, exactly
        // like the Tuple of those values (Issue #9786). Delegating reuses the I64
        // index state convention so `collect`/`for`/splat/`sum` all work.
        if let Value::NamedTuple(nt) = coll {
            return self.iterate_first(&Value::Tuple(TupleValue::new(nt.values.clone())));
        }
        // Static arrays (SVector / SMatrix) iterate over their column-major
        // backing tuple (Issue #7460, Phase 4). Delegating to the tuple path
        // reuses the I64 index state convention and keeps element types intact.
        if let Value::StaticArrayInline(sv) = coll {
            return self.iterate_first(&Value::Tuple(sv.to_tuple_value()));
        }
        if let Value::StaticArray(sv) = coll {
            return self.iterate_first(&Value::Tuple(sv.to_tuple_value()));
        }
        match coll {
            _ if is_native_array_value(coll) => {
                let arr = native_array_value_ref(coll).ok_or_else(|| {
                    VmError::TypeError(
                        "iterate: collection unexpectedly lost native Array storage".to_string(),
                    )
                })?;
                let arr_borrow = arr.borrow();
                let len = arr_borrow.element_count();
                if len == 0 {
                    Ok(Value::Nothing)
                } else {
                    let elem = arr_borrow.get_linear(0)?;
                    let state = Value::I64(2);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, state],
                    }))
                }
            }
            Value::Memory(mem) => {
                let mem_borrow = mem.borrow();
                let len = mem_borrow.len();
                if len == 0 {
                    Ok(Value::Nothing)
                } else {
                    let elem = mem_borrow.get(1)?;
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, Value::I64(2)],
                    }))
                }
            }
            Value::Range(range) => match first_range_iteration(range)? {
                None => Ok(Value::Nothing),
                Some((value, state)) => Ok(Value::Tuple(TupleValue {
                    elements: vec![value, state],
                })),
            },
            Value::Tuple(t) => {
                if t.elements.is_empty() {
                    Ok(Value::Nothing)
                } else {
                    let elem = t.elements[0].clone();
                    let state = Value::I64(2);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, state],
                    }))
                }
            }
            // Byte-offset iterate state, exact malformed Chars (Issue #8995).
            Value::Str(s) => Ok(match string_iterate_at(s.as_bytes(), 1) {
                None => Value::Nothing,
                Some((elem, state)) => Value::Tuple(TupleValue {
                    elements: vec![elem, state],
                }),
            }),
            Value::StrBytes(bytes) => Ok(match string_iterate_at(bytes.as_ref(), 1) {
                None => Value::Nothing,
                Some((elem, state)) => Value::Tuple(TupleValue {
                    elements: vec![elem, state],
                }),
            }),
            // CartesianIndices iteration support
            Value::Struct(s) if &*s.struct_name == "CartesianIndices" => {
                // Get dims from the struct
                let dims = s
                    .values
                    .first()
                    .cloned()
                    .unwrap_or(Value::Tuple(TupleValue { elements: vec![] }));
                if let Value::Tuple(dims_tuple) = dims {
                    if dims_tuple.elements.is_empty() {
                        // 0-dimensional: return single CartesianIndex(()) then done
                        let ci = StructInstance {
                            type_id: self.get_cartesian_index_type_id(),
                            struct_name: "CartesianIndex".into(),
                            values: vec![Value::Tuple(TupleValue { elements: vec![] })],
                        };
                        let state = Value::Bool(true); // Signal done after first
                        Ok(Value::Tuple(TupleValue {
                            elements: vec![Value::Struct(ci), state],
                        }))
                    } else {
                        // Check if any dimension is 0
                        for d in &dims_tuple.elements {
                            if let Value::I64(v) = d {
                                if *v <= 0 {
                                    return Ok(Value::Nothing);
                                }
                            }
                        }
                        // Start at (1, 1, ..., 1)
                        let n = dims_tuple.elements.len();
                        let first_idx: Vec<Value> = (0..n).map(|_| Value::I64(1)).collect();
                        let ci = StructInstance {
                            type_id: self.get_cartesian_index_type_id(),
                            struct_name: "CartesianIndex".into(),
                            values: vec![Value::Tuple(TupleValue {
                                elements: first_idx.clone(),
                            })],
                        };
                        // State is the current index tuple
                        let state = Value::Tuple(TupleValue {
                            elements: first_idx,
                        });
                        Ok(Value::Tuple(TupleValue {
                            elements: vec![Value::Struct(ci), state],
                        }))
                    }
                } else {
                    Err(VmError::TypeError(
                        "CartesianIndices: dims must be a Tuple".to_string(),
                    ))
                }
            }
            Value::Struct(s) if Self::is_enumerate_struct_name(&s.struct_name) => {
                self.iterate_first_enumerate_fields(&s.values)
            }
            Value::Struct(s) if Self::is_count_struct_name(&s.struct_name) => {
                self.iterate_first_count_fields(&s.values)
            }
            Value::Struct(s) if Self::is_array_wrapper_struct_name(&s.struct_name) => {
                self.iterate_array_wrapper_fields(&s.values, 0)
            }
            Value::StructRef(idx) => {
                let Some(s) = self.struct_heap.get(*idx) else {
                    return Err(VmError::TypeError(format!(
                        "iterate: unsupported struct type for StructRef({})",
                        idx
                    )));
                };
                self.iterate_first_struct_dispatch(s, coll, *idx)
            }
            Value::Generator(g) => {
                let next = self.iterate_first(g.iter.as_ref())?;
                self.apply_generator_callable_to_iterate_result(&g.callable, next)
            }
            Value::Pairs(p) => {
                let Some((name, value)) = p.data.names.first().zip(p.data.values.first()) else {
                    return Ok(Value::Nothing);
                };
                let value_type_name = self.pairs_value_element_type_name(&p.data.values);
                let pair = self.pairs_entry_value(name, value, &value_type_name);
                Ok(Value::Tuple(TupleValue {
                    elements: vec![pair, Value::I64(1)],
                }))
            }
            // Set iteration: returns each element directly
            // Scalar number iteration (Julia: iterate(x::Number) = (x, nothing))
            // Numbers iterate exactly once, yielding themselves.
            Value::Bool(_)
            | Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::F16(_)
            | Value::F32(_)
            | Value::F64(_)
            | Value::BigInt(_)
            | Value::BigFloat(_) => Ok(Value::Tuple(TupleValue {
                elements: vec![coll.clone(), Value::Nothing],
            })),
            _ => Err(VmError::TypeError(format!(
                "iterate: unsupported collection type {:?}",
                coll
            ))),
        }
    }

    /// Subsequent iteration: iterate(collection, state) -> (element, state) or nothing
    /// Subsequent state handling for builtin collection fallbacks. Native
    /// arrays and tuples use Julia-visible 1-based next indices; some legacy
    /// fallbacks keep their existing internal state convention.
    /// Dispatch `iterate(next)` for a heap `StructRef` by its struct name.
    /// Mirrors the original inline match (named arms early-return or produce a
    /// value; the catch-all errors). Extracted from `iterate_next` to keep it
    /// flat (Issue #6833).
    #[allow(clippy::too_many_arguments)]
    fn iterate_next_struct_dispatch(
        &self,
        s: &StructInstance,
        idx: usize,
        coll: &Value,
        state: &Value,
        struct_idx: usize,
    ) -> Result<Value, VmError> {
        if Self::is_array_wrapper_struct_name(&s.struct_name) {
            return if idx == 0 {
                Ok(Value::Nothing)
            } else {
                self.iterate_array_wrapper_fields(&s.values, idx - 1)
            };
        }
        match &*s.struct_name {
            name if matches!(
                Self::range_struct_base_name(name),
                "UnitRange" | "StepRange"
            ) =>
            {
                let range = Self::range_struct_as_range_value(s).ok_or_else(|| {
                    VmError::TypeError(format!("{}: invalid range struct fields", s.struct_name))
                })?;
                if idx >= range.length() as usize {
                    Ok(Value::Nothing)
                } else {
                    let elem = range.get_value(idx as i64 + 1)?;
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, Value::I64((idx + 1) as i64)],
                    }))
                }
            }
            "OneTo" => {
                if let Some(Value::I64(stop)) = s.values.first() {
                    if idx as i64 > *stop {
                        Ok(Value::Nothing)
                    } else {
                        let elem = Value::I64(idx as i64);
                        let next_state = Value::I64((idx + 1) as i64);
                        Ok(Value::Tuple(TupleValue {
                            elements: vec![elem, next_state],
                        }))
                    }
                } else {
                    Err(VmError::TypeError(
                        "OneTo: stop must be an integer".to_string(),
                    ))
                }
            }
            "EachCol" => {
                // EachCol iteration: subsequent column access.
                // Route raw ArrayValue storage matching through
                // matrix_array_dims_2d + extract_matrix_column_1based
                // (Issue #3908).
                if let Some(mat) = s.values.first() {
                    match Self::matrix_array_dims_2d(mat, "EachCol")? {
                        None => {
                            // 1D array: only one column
                            Ok(Value::Nothing)
                        }
                        Some((nrows, ncols)) => {
                            if idx > ncols {
                                return Ok(Value::Nothing);
                            }
                            let col = Self::extract_matrix_column_1based(
                                mat, idx as i64, nrows, "EachCol",
                            )?;
                            let next_state = Value::I64((idx + 1) as i64);
                            Ok(Value::Tuple(TupleValue {
                                elements: vec![col, next_state],
                            }))
                        }
                    }
                } else {
                    Err(VmError::TypeError(
                        "EachCol: missing matrix value".to_string(),
                    ))
                }
            }
            "EachRow" => {
                // EachRow iteration: subsequent row access. Routes through
                // logical dims probe + row-extraction helper (Issue #3908).
                if let Some(mat) = s.values.first() {
                    match Self::matrix_array_dims_2d(mat, "EachRow")? {
                        None => {
                            // 1D array: delegate to array iteration
                            self.iterate_next(mat, state)
                        }
                        Some((nrows, ncols)) => {
                            if idx > nrows {
                                return Ok(Value::Nothing);
                            }
                            let row =
                                Self::extract_matrix_row_1based(mat, idx as i64, ncols, "EachRow")?;
                            let next_state = Value::I64((idx + 1) as i64);
                            Ok(Value::Tuple(TupleValue {
                                elements: vec![row, next_state],
                            }))
                        }
                    }
                } else {
                    Err(VmError::TypeError(
                        "EachRow: missing matrix value".to_string(),
                    ))
                }
            }
            "EachSlice" => {
                // EachSlice iteration: subsequent slice access. Logical dims
                // probe + row/column extraction helpers keep raw ArrayValue
                // storage matching inside the value layer (Issue #3908).
                if let (Some(mat), Some(dim_val)) = (s.values.first(), s.values.get(1)) {
                    let dim = match dim_val {
                        Value::I64(d) => *d as usize,
                        _ => {
                            return Err(VmError::TypeError(
                                "EachSlice: dim must be an integer".to_string(),
                            ))
                        }
                    };
                    match Self::matrix_array_dims_2d(mat, "EachSlice")? {
                        None => {
                            if dim == 1 {
                                self.iterate_next(mat, state)
                            } else {
                                Ok(Value::Nothing)
                            }
                        }
                        Some((nrows, ncols)) => {
                            let n = if dim == 1 { nrows } else { ncols };
                            if idx > n {
                                return Ok(Value::Nothing);
                            }
                            let slice = if dim == 1 {
                                Self::extract_matrix_row_1based(
                                    mat,
                                    idx as i64,
                                    ncols,
                                    "EachSlice",
                                )?
                            } else {
                                Self::extract_matrix_column_1based(
                                    mat,
                                    idx as i64,
                                    nrows,
                                    "EachSlice",
                                )?
                            };
                            let next_state = Value::I64((idx + 1) as i64);
                            Ok(Value::Tuple(TupleValue {
                                elements: vec![slice, next_state],
                            }))
                        }
                    }
                } else {
                    Err(VmError::TypeError("EachSlice: missing values".to_string()))
                }
            }
            "SkipMissing" => {
                // SkipMissing iteration: continue iterating the inner collection, skipping missing values
                if let Some(inner_coll) = s.values.first() {
                    // Continue iterating the inner collection with the given state
                    let next = self.iterate_next(inner_coll, state)?;
                    if matches!(next, Value::Nothing) {
                        return Ok(Value::Nothing);
                    }
                    // Extract (val, newstate) from the result
                    if let Value::Tuple(t) = &next {
                        if t.elements.len() == 2 {
                            let val = &t.elements[0];
                            let newstate = &t.elements[1];
                            // Check if value is missing
                            if self.is_missing(val) {
                                // Skip this missing value and continue to next
                                return self.iterate_next(coll, newstate);
                            }
                            // Return the non-missing value with new state
                            return Ok(next);
                        }
                    }
                    Ok(next)
                } else {
                    Err(VmError::TypeError(
                        "SkipMissing: missing inner collection".to_string(),
                    ))
                }
            }
            name if name.starts_with("LinRange") => {
                // LinRange iteration: next element
                // Fields: start, stop, len, lendiv
                // State (idx) is 1-based index of the PREVIOUS element returned
                // We need to compute element at idx+1 (next element)
                if s.values.len() >= 4 {
                    let len = match &s.values[THIRD_FIELD_INDEX] {
                        Value::I64(n) => *n,
                        _ => {
                            return Err(VmError::TypeError("LinRange: len must be I64".to_string()))
                        }
                    };
                    let next_idx = idx + 1; // Compute the next element's 1-based index
                    if next_idx as i64 > len {
                        return Ok(Value::Nothing);
                    }
                    let start = match &s.values[FIRST_FIELD_INDEX] {
                        Value::F64(f) => *f,
                        Value::I64(i) => *i as f64,
                        _ => {
                            return Err(VmError::TypeError(
                                "LinRange: start must be numeric".to_string(),
                            ))
                        }
                    };
                    let stop = match &s.values[SECOND_FIELD_INDEX] {
                        Value::F64(f) => *f,
                        Value::I64(i) => *i as f64,
                        _ => {
                            return Err(VmError::TypeError(
                                "LinRange: stop must be numeric".to_string(),
                            ))
                        }
                    };
                    let lendiv = match &s.values[FOURTH_FIELD_INDEX] {
                        Value::I64(n) => *n,
                        _ => {
                            return Err(VmError::TypeError(
                                "LinRange: lendiv must be I64".to_string(),
                            ))
                        }
                    };
                    // lerp formula: (1 - t) * start + t * stop where t = (next_idx - 1) / lendiv
                    let t = (next_idx as f64 - 1.0) / lendiv as f64;
                    let elem = (1.0 - t) * start + t * stop;
                    let next_state = Value::I64(next_idx as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![Value::F64(elem), next_state],
                    }))
                } else {
                    Err(VmError::TypeError(
                        "LinRange: invalid struct fields".to_string(),
                    ))
                }
            }
            name if name.starts_with("StepRangeLen") => {
                // StepRangeLen iteration: next element
                // Fields: ref, step, len, offset
                // State (idx) is 1-based index of the PREVIOUS element returned
                // We need to compute element at idx+1 (next element)
                if s.values.len() >= 4 {
                    let len = match &s.values[THIRD_FIELD_INDEX] {
                        Value::I64(n) => *n,
                        _ => {
                            return Err(VmError::TypeError(
                                "StepRangeLen: len must be I64".to_string(),
                            ))
                        }
                    };
                    let next_idx = idx + 1; // Compute the next element's 1-based index
                    if next_idx as i64 > len {
                        return Ok(Value::Nothing);
                    }
                    let ref_val = match &s.values[FIRST_FIELD_INDEX] {
                        Value::F64(f) => *f,
                        Value::I64(i) => *i as f64,
                        _ => {
                            return Err(VmError::TypeError(
                                "StepRangeLen: ref must be numeric".to_string(),
                            ))
                        }
                    };
                    let step_val = match &s.values[SECOND_FIELD_INDEX] {
                        Value::F64(f) => *f,
                        Value::I64(i) => *i as f64,
                        _ => {
                            return Err(VmError::TypeError(
                                "StepRangeLen: step must be numeric".to_string(),
                            ))
                        }
                    };
                    let offset = match &s.values[FOURTH_FIELD_INDEX] {
                        Value::I64(n) => *n,
                        _ => {
                            return Err(VmError::TypeError(
                                "StepRangeLen: offset must be I64".to_string(),
                            ))
                        }
                    };
                    // Element at index next_idx: ref + (next_idx - offset) * step
                    let elem = ref_val + (next_idx as f64 - offset as f64) * step_val;
                    let next_state = Value::I64(next_idx as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![Value::F64(elem), next_state],
                    }))
                } else {
                    Err(VmError::TypeError(
                        "StepRangeLen: invalid struct fields".to_string(),
                    ))
                }
            }
            name if name.starts_with("LogRange") => {
                // LogRange iteration: next element.
                // State (idx) is the 1-based index previously returned.
                if s.values.len() >= 5 {
                    let len = match &s.values[THIRD_FIELD_INDEX] {
                        Value::I64(n) => *n,
                        _ => {
                            return Err(VmError::TypeError("LogRange: len must be I64".to_string()))
                        }
                    };
                    let next_idx = idx + 1;
                    if next_idx as i64 > len {
                        return Ok(Value::Nothing);
                    }
                    let stop = match &s.values[SECOND_FIELD_INDEX] {
                        Value::F64(f) => *f,
                        Value::I64(i) => *i as f64,
                        _ => {
                            return Err(VmError::TypeError(
                                "LogRange: stop must be numeric".to_string(),
                            ))
                        }
                    };
                    let elem = if next_idx as i64 == len {
                        stop
                    } else {
                        let log_start_div = match &s.values[FOURTH_FIELD_INDEX] {
                            Value::F64(f) => *f,
                            Value::I64(i) => *i as f64,
                            _ => {
                                return Err(VmError::TypeError(
                                    "LogRange: log_start_div must be numeric".to_string(),
                                ))
                            }
                        };
                        let log_stop_div = match &s.values[4] {
                            Value::F64(f) => *f,
                            Value::I64(i) => *i as f64,
                            _ => {
                                return Err(VmError::TypeError(
                                    "LogRange: log_stop_div must be numeric".to_string(),
                                ))
                            }
                        };
                        let i = next_idx as f64;
                        let len_f = len as f64;
                        ((len_f - i) * log_start_div + (i - 1.0) * log_stop_div).exp()
                    };
                    let next_state = Value::I64(next_idx as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![Value::F64(elem), next_state],
                    }))
                } else {
                    Err(VmError::TypeError(
                        "LogRange: invalid struct fields".to_string(),
                    ))
                }
            }
            _ => Err(VmError::TypeError(format!(
                "iterate: unsupported struct type for StructRef({})",
                struct_idx
            ))),
        }
    }

    pub(in crate::vm) fn iterate_next(
        &self,
        coll: &Value,
        state: &Value,
    ) -> Result<Value, VmError> {
        // Core.SimpleVector iterates exactly like a Tuple (Issue #4722).
        if let Value::SimpleVector(sv) = coll {
            return self.iterate_next(&Value::Tuple(sv.clone()), state);
        }
        // A NamedTuple iterates over its field values, matching the iterate_first
        // delegation above (Issue #9786).
        if let Value::NamedTuple(nt) = coll {
            return self.iterate_next(&Value::Tuple(TupleValue::new(nt.values.clone())), state);
        }
        // Static arrays delegate to their column-major backing tuple (Issue
        // #7460, Phase 4), matching the iterate_first delegation above.
        if let Value::StaticArrayInline(sv) = coll {
            return self.iterate_next(&Value::Tuple(sv.to_tuple_value()), state);
        }
        if let Value::StaticArray(sv) = coll {
            return self.iterate_next(&Value::Tuple(sv.to_tuple_value()), state);
        }
        // Scalar number iteration (Julia: iterate(x::Number, ::Nothing) = nothing)
        // After yielding once, scalar iteration is done.
        if matches!(
            coll,
            Value::Bool(_)
                | Value::I8(_)
                | Value::I16(_)
                | Value::I32(_)
                | Value::I64(_)
                | Value::I128(_)
                | Value::U8(_)
                | Value::U16(_)
                | Value::U32(_)
                | Value::U64(_)
                | Value::U128(_)
                | Value::F16(_)
                | Value::F32(_)
                | Value::F64(_)
                | Value::BigInt(_)
                | Value::BigFloat(_)
        ) && matches!(state, Value::Nothing)
        {
            return Ok(Value::Nothing);
        }

        // Handle CartesianIndices with Tuple or Bool state
        if let Some(dims) = self.get_cartesian_indices_dims(coll) {
            return self.iterate_next_cartesian_indices(&dims, state);
        }

        if let Value::Generator(g) = coll {
            let next = self.iterate_next(g.iter.as_ref(), state)?;
            return self.apply_generator_callable_to_iterate_result(&g.callable, next);
        }

        if let Value::StructRef(struct_idx) = coll {
            if let Some(s) = self.struct_heap.get(*struct_idx) {
                if Self::is_zip_struct_name(&s.struct_name) {
                    return self.iterate_next_zip_fields(&s.values, state);
                }
                if Self::is_enumerate_struct_name(&s.struct_name) {
                    return self.iterate_next_enumerate_fields(&s.values, state);
                }
                if Self::is_count_struct_name(&s.struct_name) {
                    return self.iterate_next_count_fields(&s.values, state);
                }
            }
        }
        if let Value::Struct(s) = coll {
            if Self::is_enumerate_struct_name(&s.struct_name) {
                return self.iterate_next_enumerate_fields(&s.values, state);
            }
            if Self::is_count_struct_name(&s.struct_name) {
                return self.iterate_next_count_fields(&s.values, state);
            }
        }

        if let Value::Range(range) = coll {
            return match next_range_iteration(range, state)? {
                None => Ok(Value::Nothing),
                Some((value, next_state)) => Ok(Value::Tuple(TupleValue {
                    elements: vec![value, next_state],
                })),
            };
        }

        let idx = match state {
            Value::I64(i) => *i as usize,
            _ => return Err(VmError::TypeError("iterate: state must be I64".to_string())),
        };

        match coll {
            _ if is_native_array_value(coll) => {
                let arr = native_array_value_ref(coll).ok_or_else(|| {
                    VmError::TypeError(
                        "iterate: collection unexpectedly lost native Array storage".to_string(),
                    )
                })?;
                let arr_borrow = arr.borrow();
                if idx == 0 || idx > arr_borrow.element_count() {
                    Ok(Value::Nothing)
                } else {
                    let elem = arr_borrow.get_linear(idx - 1)?;
                    let next_state = Value::I64((idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, next_state],
                    }))
                }
            }
            Value::Memory(mem) => {
                let mem_borrow = mem.borrow();
                if idx == 0 || idx > mem_borrow.len() {
                    Ok(Value::Nothing)
                } else {
                    let elem = mem_borrow.get(idx)?;
                    let next_state = Value::I64((idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, next_state],
                    }))
                }
            }
            Value::Tuple(t) => {
                if idx == 0 || idx > t.elements.len() {
                    Ok(Value::Nothing)
                } else {
                    let elem = t.elements[idx - 1].clone();
                    let next_state = Value::I64((idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, next_state],
                    }))
                }
            }
            // Byte-offset iterate state, exact malformed Chars, linear
            // per-step decode (Issue #8995).
            Value::Str(s) => Ok(match string_iterate_at(s.as_bytes(), idx as i64) {
                None => Value::Nothing,
                Some((elem, next_state)) => Value::Tuple(TupleValue {
                    elements: vec![elem, next_state],
                }),
            }),
            Value::StrBytes(bytes) => Ok(match string_iterate_at(bytes.as_ref(), idx as i64) {
                None => Value::Nothing,
                Some((elem, next_state)) => Value::Tuple(TupleValue {
                    elements: vec![elem, next_state],
                }),
            }),
            Value::StructRef(struct_idx) => {
                let Some(s) = self.struct_heap.get(*struct_idx) else {
                    return Err(VmError::TypeError(format!(
                        "iterate: invalid StructRef({})",
                        struct_idx
                    )));
                };
                self.iterate_next_struct_dispatch(s, idx, coll, state, *struct_idx)
            }
            Value::Pairs(p) => {
                if idx >= p.data.values.len() {
                    return Ok(Value::Nothing);
                }
                let Some(name) = p.data.names.get(idx) else {
                    return Ok(Value::Nothing);
                };
                let value = &p.data.values[idx];
                let value_type_name = self.pairs_value_element_type_name(&p.data.values);
                let pair = self.pairs_entry_value(name, value, &value_type_name);
                Ok(Value::Tuple(TupleValue {
                    elements: vec![pair, Value::I64((idx + 1) as i64)],
                }))
            }
            // Set iteration: returns each element directly
            _ => Err(VmError::TypeError(format!(
                "iterate: unsupported collection type {:?}",
                coll
            ))),
        }
    }

    /// Helper: extract dims from CartesianIndices (Value::Struct or Value::StructRef)
    fn get_cartesian_indices_dims(&self, coll: &Value) -> Option<Vec<i64>> {
        match coll {
            Value::Struct(s) if &*s.struct_name == "CartesianIndices" => {
                if let Some(Value::Tuple(dims_tuple)) = s.values.first() {
                    Some(
                        dims_tuple
                            .elements
                            .iter()
                            .filter_map(|d| {
                                if let Value::I64(v) = d {
                                    Some(*v)
                                } else {
                                    None
                                }
                            })
                            .collect(),
                    )
                } else {
                    None
                }
            }
            Value::StructRef(idx) => {
                let s = self.struct_heap.get(*idx)?;
                if &*s.struct_name != "CartesianIndices" {
                    return None;
                }
                let Some(Value::Tuple(dims_tuple)) = s.values.first() else {
                    return None;
                };
                Some(
                    dims_tuple
                        .elements
                        .iter()
                        .filter_map(|d| match d {
                            Value::I64(v) => Some(*v),
                            _ => None,
                        })
                        .collect(),
                )
            }
            _ => None,
        }
    }

    /// Helper: iterate_next for CartesianIndices
    fn iterate_next_cartesian_indices(
        &self,
        dims: &[i64],
        state: &Value,
    ) -> Result<Value, VmError> {
        // Handle 0-dimensional case (state is Bool)
        if let Value::Bool(true) = state {
            // Already iterated over the single element
            return Ok(Value::Nothing);
        }

        // State is a Tuple of current indices
        let current_indices: Vec<i64> = match state {
            Value::Tuple(t) => t
                .elements
                .iter()
                .filter_map(|e| {
                    if let Value::I64(v) = e {
                        Some(*v)
                    } else {
                        None
                    }
                })
                .collect(),
            _ => {
                return Err(VmError::TypeError(
                    "CartesianIndices state must be Tuple or Bool".to_string(),
                ))
            }
        };

        if current_indices.len() != dims.len() {
            return Err(VmError::TypeError(
                "CartesianIndices state dimension mismatch".to_string(),
            ));
        }

        // Increment indices in column-major order (first index varies fastest)
        let mut next_indices = current_indices.clone();
        let mut carry = true;

        for i in 0..dims.len() {
            if carry {
                next_indices[i] += 1;
                if next_indices[i] > dims[i] {
                    next_indices[i] = 1;
                    // carry remains true
                } else {
                    carry = false;
                }
            }
        }

        // If carry is still true, we've exhausted all indices
        if carry {
            return Ok(Value::Nothing);
        }

        // Create the CartesianIndex with the new indices
        let idx_values: Vec<Value> = next_indices.iter().map(|&v| Value::I64(v)).collect();
        let ci = StructInstance {
            type_id: self.get_cartesian_index_type_id(),
            struct_name: "CartesianIndex".into(),
            values: vec![Value::Tuple(TupleValue {
                elements: idx_values.clone(),
            })],
        };
        let new_state = Value::Tuple(TupleValue {
            elements: idx_values,
        });

        Ok(Value::Tuple(TupleValue {
            elements: vec![Value::Struct(ci), new_state],
        }))
    }

    /// Collect iterator into Array
    /// Type-preserving: returns Int64 array for integer ranges/tuples, Float64 otherwise.
    /// For arrays, creates a shallow copy (new array with same element type).
    pub(in crate::vm) fn collect_iterator(&mut self, iter: &Value) -> Result<Value, VmError> {
        match iter {
            // Array - create a type-preserving copy (not just clone the reference)
            _ if is_native_array_value(iter) => {
                // CollectFallback: array-copy-materialization
                let arr = native_array_value_ref(iter).ok_or_else(|| {
                    VmError::TypeError(
                        "collect: iterator unexpectedly lost native Array storage".to_string(),
                    )
                })?;
                let copied = {
                    let borrowed = arr.borrow();
                    ArrayValue::memory_first_copy_from_array(&borrowed)?
                };
                self.array_wrapper_value(copied)
            }
            // Range -> Array (type-preserving via RangeValue::collect())
            Value::Range(r) => {
                // CollectFallback: range-value-materialization
                self.array_wrapper_value(r.collect())
            }
            // Tuple -> Array (type-preserving)
            Value::Tuple(t) => {
                // CollectFallback: tuple-value-materialization
                let arr = ArrayValue::memory_first_collect_typejoin_values(
                    t.elements.clone(),
                    ArrayElementType::Any,
                )?;
                self.array_wrapper_value(arr)
            }
            // Core.SimpleVector -> Vector{Any} (Issue #5196). Unlike a Tuple,
            // upstream `collect(::Core.SimpleVector)` always yields `Vector{Any}`
            // (`eltype` is `Any`), so we force the `Any` element type and never
            // narrow to a concrete/numeric element type even when the elements
            // happen to be homogeneous. This preserves heterogeneous elements
            // (e.g. type parameters from `<Type>.parameters`).
            Value::SimpleVector(sv) => {
                // CollectFallback: simplevector-value-materialization
                let mut arr = ArrayValue::memory_first_with_capacity(
                    ArrayElementType::Any,
                    sv.elements.len(),
                );
                for value in &sv.elements {
                    arr.push(value.clone())?;
                }
                self.array_wrapper_value(arr)
            }
            Value::Memory(mem) => {
                // CollectFallback: memory-value-materialization
                let (values, element_type) = {
                    let borrowed = mem.borrow();
                    let mut values = Vec::with_capacity(borrowed.len());
                    for idx in 1..=borrowed.len() {
                        values.push(borrowed.get(idx)?);
                    }
                    (values, borrowed.element_type().clone())
                };
                let arr = ArrayValue::memory_first_collect_typejoin_values(values, element_type)?;
                self.array_wrapper_value(arr)
            }
            Value::Pairs(p) => {
                let value_type_name = self.pairs_value_element_type_name(&p.data.values);
                let mut arr = ArrayValue::memory_first_with_capacity(
                    ArrayElementType::Abstract(format!("Pair{{Symbol, {value_type_name}}}")),
                    p.data.values.len(),
                );
                for (name, value) in p.data.names.iter().zip(p.data.values.iter()) {
                    arr.push(self.pairs_entry_value(name, value, &value_type_name))?;
                }
                self.array_wrapper_value(arr)
            }
            Value::Struct(s) => {
                // Issue #9200 (S4a): a `ProductIterator` / `Product` (the base of a
                // desugared comma product generator) materializes column-major AND
                // recovers its N-D shape, so `collect` yields a Matrix.
                if Self::is_product_iterator_struct_name(&s.struct_name) {
                    let struct_name = s.struct_name.clone();
                    let fields = s.values.clone();
                    return self.collect_product_iterator(iter, &struct_name, &fields);
                }
                // CollectFallback: struct-native-iterator-materialization
                match self.collect_struct_iterator(s)? {
                    Some(result) => Ok(result),
                    // Issue #9127: Dict / KeySet / ValueIterator (and any other
                    // struct whose iteration is defined in pure Julia) are not
                    // among the builtin fast paths above. Drive their pure-Julia
                    // `iterate` protocol re-entrantly so a lazy generator over a
                    // Dict collects instead of erroring.
                    None => self.collect_iterator_via_iterate_protocol(iter),
                }
            }
            Value::StructRef(idx) => {
                let Some(instance) = self.struct_heap.get(*idx).cloned() else {
                    return Err(VmError::TypeError(format!(
                        "collect: invalid StructRef({})",
                        idx
                    )));
                };
                // Issue #9200 (S4a): see the `Value::Struct` arm — a product base
                // recovers its N-D shape for the product generator's Matrix result.
                if Self::is_product_iterator_struct_name(&instance.struct_name) {
                    return self.collect_product_iterator(
                        iter,
                        &instance.struct_name,
                        &instance.values,
                    );
                }
                match self.collect_struct_iterator(&instance)? {
                    Some(result) => Ok(result),
                    // Issue #9127: see the `Value::Struct` arm — a StructRef Dict
                    // materializes through its pure-Julia `iterate` protocol.
                    None => self.collect_iterator_via_iterate_protocol(iter),
                }
            }
            // Generator passed as a plain iterable to the synchronous collect
            // boundary (Issue #5138). This happens when an *eager* generator
            // expression `(expr for x in it)` (whose body is not a plain unary
            // call) is consumed by `map`, a comprehension, or another generator
            // lowered to `collect(Generator(f, gen))`. An eager generator already
            // holds its materialized values in `g.iter`, so collecting it just
            // re-materializes that array.
            //
            // Deferred consumer retirement (Issue #9200 S6): the generator collect
            // path (here + `collect_generator` in `call_dynamic.rs`) is retained as
            // a measured fast path — it FUSES nested lazy generators (S2) and
            // recovers a product iterator's N-D shape / eltype (S4) that the generic
            // iterate-based `collect` does not. Retiring it deoptimizes a hot path
            // and changes shape/eltype, so it is gated on S6's measured decision
            // (Performance Decision Protocol, Issue #9129), not this slice.
            Value::Generator(g) if matches!(g.callable, GeneratorCallable::Eager) => {
                // CollectFallback: eager-generator-iterable-materialization
                self.collect_iterator(g.iter.as_ref())
            }
            // Function-backed LAZY generator at a synchronous collect boundary
            // (Issue #9103) — e.g. a `quote` splat's `ExprNewWithSplat` builtin
            // materializing `$((:($b = 1) for b in bs)...)`. Preferred consumers
            // go through the asynchronous `collect_generator` HOF path, but a
            // builtin that needs the values mid-instruction cannot yield. Drive
            // the generator's own iterate protocol re-entrantly per element so
            // filtered callables use the same predicate/map machinery as
            // `for`/`iterate` consumers instead of projecting `g.f` to a direct
            // callable value (Issue #9405).
            Value::Generator(g) => self.collect_generator_iterate_protocol(g),
            _ => Err(VmError::TypeError(format!(
                "collect: unsupported iterator type {:?}",
                iter
            ))),
        }
    }

    /// Materialize a struct iterator handled by a builtin fast path.
    ///
    /// Returns `Ok(Some(array))` when `struct_name` names a recognized builtin
    /// iterator (array wrapper, `Zip*`, `Enumerate`, `Rest`, `LogRange`), or
    /// `Ok(None)` when it is not one of those — the caller then drives the
    /// value's pure-Julia `iterate` protocol (Issue #9127).
    fn collect_struct_iterator(
        &mut self,
        instance: &StructInstance,
    ) -> Result<Option<Value>, VmError> {
        let struct_name = &*instance.struct_name;
        let fields = &instance.values;
        if Self::is_array_wrapper_struct_name(struct_name) {
            return self.collect_array_wrapper_fields(fields).map(Some);
        }
        if matches!(
            struct_name,
            "Zip" | "Zip3" | "Zip4" | "Zip5" | "Zip6" | "Zip7"
        ) || struct_name.starts_with("Zip{")
            || struct_name.starts_with("Zip3{")
            || struct_name.starts_with("Zip4{")
            || struct_name.starts_with("Zip5{")
            || struct_name.starts_with("Zip6{")
            || struct_name.starts_with("Zip7{")
        {
            // CollectFallback: zip-struct-materialization
            return self.collect_zip_fields(fields).map(Some);
        }
        if struct_name == "Enumerate" || struct_name.starts_with("Enumerate{") {
            // CollectFallback: enumerate-struct-legacy-materialization
            return self.collect_enumerate_fields(fields).map(Some);
        }
        if struct_name == "Rest" || struct_name.starts_with("Rest{") {
            // CollectFallback: rest-struct-legacy-materialization
            return self.collect_rest_fields(fields).map(Some);
        }
        if struct_name == "LogRange" || struct_name.starts_with("LogRange{") {
            // CollectFallback: logrange-struct-materialization
            return self.collect_logrange_fields(fields).map(Some);
        }
        Ok(None)
    }

    /// Whether `struct_name` is a cartesian-product iterator — `Product{A,B}`
    /// (2-ary) or `ProductIterator` (N-ary, `src/julia/base/iterators.jl`). These
    /// are the base of a desugared comma product generator (Issue #9200 S4a).
    fn is_product_iterator_struct_name(struct_name: &str) -> bool {
        struct_name == "Product"
            || struct_name == "ProductIterator"
            || struct_name.starts_with("Product{")
            || struct_name.starts_with("ProductIterator{")
    }

    /// Materialize a `Product` / `ProductIterator` (Issue #9200 S4a), recovering
    /// its N-D shape so `collect(f(x,y) for x in a, y in b)` yields a `Matrix`.
    ///
    /// The elements are produced column-major by the pure-Julia `iterate`
    /// protocol (first component changes fastest), exactly the column-major fill
    /// order of an N-D array, so the flat values are reshaped to the product's
    /// dims (`size(::ProductIterator)`) in place. A component whose size is not
    /// natively measurable (or any arity/shape mismatch) leaves the result a flat
    /// `Vector`, matching upstream's `SizeUnknown()` fallback.
    fn collect_product_iterator(
        &mut self,
        iter: &Value,
        struct_name: &str,
        fields: &[Value],
    ) -> Result<Value, VmError> {
        let iterate_fn = Value::Function(FunctionValue::new("iterate"));
        let mut values = Vec::new();
        let mut next = self.invoke_iterate_protocol_step(&iterate_fn, vec![iter.clone()])?;
        loop {
            match next {
                Value::Nothing => break,
                Value::Tuple(tuple) if tuple.elements.len() == 2 => {
                    values.push(tuple.elements[0].clone());
                    let state = tuple.elements[1].clone();
                    next =
                        self.invoke_iterate_protocol_step(&iterate_fn, vec![iter.clone(), state])?;
                }
                other => {
                    return Err(VmError::TypeError(format!(
                        "collect(product): iterate must return `nothing` or a 2-tuple, got {:?}",
                        other.runtime_type()
                    )));
                }
            }
        }
        let count = values.len();
        let mut arr =
            ArrayValue::memory_first_collect_typejoin_values(values, ArrayElementType::Any)?;
        if let Some(dims) = self.product_component_dims(struct_name, fields) {
            let total: usize = dims.iter().product();
            if dims.len() >= 2 && total == count {
                arr.shape = dims;
            }
        }
        self.array_wrapper_value(arr)
    }

    /// The product's shape as the concatenation of each component iterable's
    /// native dims — `Product{A,B}` stores its two components as fields `a`, `b`;
    /// `ProductIterator` stores an N-tuple of components in a single field. Returns
    /// `None` if any component's size is not natively measurable (→ flat `Vector`).
    fn product_component_dims(&self, struct_name: &str, fields: &[Value]) -> Option<Vec<usize>> {
        let is_product_iterator =
            struct_name == "ProductIterator" || struct_name.starts_with("ProductIterator{");
        let mut dims = Vec::new();
        if is_product_iterator {
            let Some(Value::Tuple(tuple)) = fields.first() else {
                return None;
            };
            for component in &tuple.elements {
                dims.extend(self.native_iterable_dims(component)?);
            }
        } else {
            for component in fields {
                dims.extend(self.native_iterable_dims(component)?);
            }
        }
        Some(dims)
    }

    /// Natively measurable dims of one product component (a `Range` / `Array` /
    /// `Memory` / `Tuple` / public `Array{T,N}` wrapper). A multi-dim array
    /// contributes multiple dims, mirroring upstream `size(::ProductIterator)`
    /// which concatenates each component's `size`. Returns `None` for components
    /// whose size needs the pure-Julia iterator-size trait.
    fn native_iterable_dims(&self, v: &Value) -> Option<Vec<usize>> {
        if is_native_array_value(v) {
            return native_array_value_ref(v).map(|a| a.borrow().shape.clone());
        }
        match v {
            Value::Range(r) => Some(vec![r.len()]),
            Value::Struct(s) => {
                if let Some(range) = Self::range_struct_as_range_value(s) {
                    Some(vec![range.length().max(0) as usize])
                } else {
                    match array_wrapper_value_to_array_value(v, &self.struct_heap) {
                        Ok(Some(arr)) => Some(arr.shape.clone()),
                        _ => None,
                    }
                }
            }
            Value::StructRef(idx) => {
                if let Some(range) = self
                    .struct_heap
                    .get(*idx)
                    .and_then(Self::range_struct_as_range_value)
                {
                    Some(vec![range.length().max(0) as usize])
                } else {
                    match array_wrapper_value_to_array_value(v, &self.struct_heap) {
                        Ok(Some(arr)) => Some(arr.shape.clone()),
                        _ => None,
                    }
                }
            }
            Value::Memory(m) => Some(vec![m.borrow().len()]),
            Value::Tuple(t) => Some(vec![t.elements.len()]),
            _ => match array_wrapper_value_to_array_value(v, &self.struct_heap) {
                Ok(Some(arr)) => Some(arr.shape.clone()),
                _ => None,
            },
        }
    }

    /// Materialize an arbitrary iterable by driving its pure-Julia `iterate`
    /// protocol re-entrantly (Issue #9127).
    ///
    /// The synchronous `collect_iterator` fast paths only cover the VM's builtin
    /// carriers. `Dict` / `KeySet` / `ValueIterator` — and any user-defined
    /// iterable struct — define `iterate` in pure Julia, so materializing them
    /// requires stepping the interpreter (exactly what a `for` loop does). This
    /// mirrors the function-backed lazy-generator collect path just below:
    /// each `iterate` call may start a frame, which we drive to completion with
    /// `run_until_frame_return`.
    ///
    /// When no `iterate` method applies, the original
    /// `collect: unsupported iterator type ...` error is preserved so genuinely
    /// non-iterable values still fail the same way.
    pub(in crate::vm) fn collect_iterator_via_iterate_protocol(
        &mut self,
        iter: &Value,
    ) -> Result<Value, VmError> {
        if self
            .find_best_method_index(&["iterate"], std::slice::from_ref(iter))
            .is_none()
        {
            return Err(VmError::TypeError(format!(
                "collect: unsupported iterator type {}",
                self.get_type_name(iter)
            )));
        }

        let iterate_fn = Value::Function(FunctionValue::new("iterate"));
        let mut values = Vec::new();
        let mut next = self.invoke_iterate_protocol_step(&iterate_fn, vec![iter.clone()])?;
        loop {
            match next {
                Value::Nothing => break,
                Value::Tuple(tuple) if tuple.elements.len() == 2 => {
                    let element = tuple.elements[0].clone();
                    let state = tuple.elements[1].clone();
                    values.push(element);
                    next =
                        self.invoke_iterate_protocol_step(&iterate_fn, vec![iter.clone(), state])?;
                }
                other => {
                    return Err(VmError::TypeError(format!(
                        "collect: iterate must return `nothing` or a 2-tuple, got {:?}",
                        other.runtime_type()
                    )));
                }
            }
        }
        let arr = ArrayValue::memory_first_collect_typejoin_values(values, ArrayElementType::Any)?;
        self.array_wrapper_value(arr)
    }

    /// Run one generic-function step without letting synchronous interpreter
    /// re-entry consume an ancestor handler (Issue #11372).
    fn sync_splat_callable_step(
        &mut self,
        function_name: &str,
        args: Vec<Value>,
    ) -> Result<SplatPreparation<Value>, VmError> {
        use crate::vm::hof_exec::state::RuntimeCallableResult;

        let target_depth = self.frames.len();
        let saved_floor = self.eval_dispatch_floor;
        self.eval_dispatch_floor = Some(target_depth);
        let function = Value::Function(FunctionValue::new(function_name));
        let result = match self.call_runtime_callable_value(function, args) {
            Ok(RuntimeCallableResult::Immediate(value)) => Ok(SplatPreparation::Ready(value)),
            Ok(RuntimeCallableResult::StartedFrame) => self
                .run_until_frame_return(target_depth)
                .map(SplatPreparation::Ready),
            Ok(RuntimeCallableResult::Raised) => Ok(SplatPreparation::Raised),
            Err(err) => Err(err),
        };
        self.eval_dispatch_floor = saved_floor;
        result
    }

    fn missing_splat_method_error(&self, function_name: &str, args: &[Value]) -> VmError {
        let arg_types = args
            .iter()
            .map(|value| self.get_type_name(value))
            .collect::<Vec<_>>()
            .join(", ");
        VmError::MethodError(format!(
            "no method matching {}({})",
            function_name, arg_types
        ))
    }

    fn native_splat_iterate_step(&self, args: &[Value]) -> Option<Result<Value, VmError>> {
        let result = match args {
            [collection] => self.iterate_first(collection),
            [collection, state] => self.iterate_next(collection, state),
            _ => return None,
        };
        match &result {
            Err(VmError::TypeError(message))
                if message.starts_with("iterate: unsupported collection type")
                    || message.starts_with("iterate: unsupported struct type")
                    || message == "iterate: state must be I64" =>
            {
                None
            }
            _ => Some(result),
        }
    }

    fn splat_iterate_step(&mut self, args: Vec<Value>) -> Result<SplatPreparation<Value>, VmError> {
        if let Some(Value::StructRef(index)) = args.first() {
            if self.struct_heap.get(*index).is_none() {
                return Err(VmError::InternalError(format!(
                    "Invalid struct reference: index {} out of bounds",
                    index
                )));
            }
        }
        if self.find_best_method_index(&["iterate"], &args).is_some() {
            return self.sync_splat_callable_step("iterate", args);
        }
        if let Some(result) = self.native_splat_iterate_step(&args) {
            return result.map(SplatPreparation::Ready);
        }
        Err(self.missing_splat_method_error("iterate", &args))
    }

    fn native_splat_indexed_iterate_step(&self, args: &[Value]) -> Result<Option<Value>, VmError> {
        let [entry, Value::I64(index), rest @ ..] = args else {
            return Ok(None);
        };
        if rest.len() > 1 || *index < 1 {
            return Ok(None);
        }
        let zero_based =
            usize::try_from(*index - 1).map_err(|_| VmError::TupleIndexOutOfBounds {
                index: *index,
                length: 0,
            })?;

        let field = match entry {
            Value::Tuple(_) | Value::NamedTuple(_) => {
                Some(self.julia_nth_field_checked(entry, zero_based)?)
            }
            _ if is_native_array_value(entry) => {
                let array = native_array_value_ref(entry).ok_or_else(|| {
                    VmError::InternalError("indexed_iterate lost native Array storage".to_string())
                })?;
                let array = array.borrow();
                if zero_based >= array.element_count() {
                    return Err(VmError::TupleIndexOutOfBounds {
                        index: *index,
                        length: array.element_count(),
                    });
                }
                Some(array.get_linear(zero_based)?)
            }
            _ => match array_wrapper_value_to_array_value(entry, &self.struct_heap)? {
                Some(array) => {
                    if zero_based >= array.element_count() {
                        return Err(VmError::TupleIndexOutOfBounds {
                            index: *index,
                            length: array.element_count(),
                        });
                    }
                    Some(array.get_linear(zero_based)?)
                }
                None => None,
            },
        };
        Ok(field.map(|field| {
            Value::Tuple(TupleValue::new(vec![
                field,
                Value::I64(index.saturating_add(1)),
            ]))
        }))
    }

    fn splat_indexed_iterate_step(
        &mut self,
        args: Vec<Value>,
    ) -> Result<SplatPreparation<Value>, VmError> {
        if self
            .find_best_method_index(&["indexed_iterate", "Base.indexed_iterate"], &args)
            .is_some()
        {
            return self.sync_splat_callable_step("indexed_iterate", args);
        }
        if let Some(result) = self.native_splat_indexed_iterate_step(&args)? {
            return Ok(SplatPreparation::Ready(result));
        }

        // Pure Julia's generic `indexed_iterate(I, i[, state])` delegates to
        // `iterate(I[, state])`; only specialized carriers above project
        // directly. Preserve the requested field index for the BoundsError
        // when the delegated iterator is exhausted.
        let Some(Value::I64(index)) = args.get(1) else {
            return Err(self.missing_splat_method_error("indexed_iterate", &args));
        };
        let mut iterate_args = vec![args[0].clone()];
        if let Some(state) = args.get(2) {
            iterate_args.push(state.clone());
        }
        match self.splat_iterate_step(iterate_args)? {
            SplatPreparation::Ready(Value::Nothing) => Err(VmError::TupleIndexOutOfBounds {
                index: *index,
                length: usize::try_from(index.saturating_sub(1)).unwrap_or(0),
            }),
            SplatPreparation::Ready(result) => Ok(SplatPreparation::Ready(result)),
            SplatPreparation::Raised => Ok(SplatPreparation::Raised),
        }
    }

    fn next_generic_splat_value(
        &mut self,
        cursor: &mut SplatValueCursor,
    ) -> Result<SplatPreparation<Option<Value>>, VmError> {
        match cursor {
            SplatValueCursor::Generic { collection, state } => {
                let mut args = vec![self.clone_transient_root(*collection)?];
                if let Some(state) = state {
                    args.push(self.clone_transient_root(*state)?);
                }
                let next = self.splat_iterate_step(args)?;
                let current = match next {
                    SplatPreparation::Ready(current) => current,
                    SplatPreparation::Raised => return Ok(SplatPreparation::Raised),
                };
                if matches!(current, Value::Nothing) {
                    return Ok(SplatPreparation::Ready(None));
                }
                // No Julia call or GC safepoint occurs while projecting the two
                // physical fields. Keep only the next state rooted across the
                // following `iterate` call; the whole step result is scratch.
                let element = self.julia_nth_field_checked(&current, 0)?;
                let next_state = self.julia_nth_field_checked(&current, 1)?;
                let next_state = match state {
                    Some(state) => {
                        self.replace_transient_root(*state, next_state)?;
                        *state
                    }
                    None => self.push_transient_root(next_state)?,
                };
                *state = Some(next_state);
                Ok(SplatPreparation::Ready(Some(element)))
            }
        }
    }

    fn collect_generic_splat_root(
        &mut self,
        value: TransientRootId,
    ) -> Result<SplatPreparation<Vec<TransientRootId>>, VmError> {
        let mut cursor = SplatValueCursor::Generic {
            collection: value,
            state: None,
        };
        let mut values = Vec::new();
        loop {
            match self.next_generic_splat_value(&mut cursor)? {
                SplatPreparation::Ready(Some(value)) => {
                    // The produced element becomes part of the final expanded
                    // argument list, so unlike the step container it must stay
                    // rooted until target dispatch.
                    values.push(self.push_transient_root(value)?);
                }
                SplatPreparation::Ready(None) => {
                    return Ok(SplatPreparation::Ready(values));
                }
                SplatPreparation::Raised => return Ok(SplatPreparation::Raised),
            }
        }
    }

    fn collect_positional_splat_root(
        &mut self,
        value: TransientRootId,
    ) -> Result<SplatPreparation<Vec<TransientRootId>>, VmError> {
        let current = self.clone_transient_root(value)?;
        if let Some(values) = try_expand_splat_value_with_heap(&current, &self.struct_heap)? {
            let values = values
                .into_iter()
                .map(|value| self.push_transient_root(value))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(SplatPreparation::Ready(values));
        }
        self.collect_generic_splat_root(value)
    }

    fn put_transient_root_slot(
        &mut self,
        slot: &mut Option<TransientRootId>,
        value: Value,
    ) -> Result<TransientRootId, VmError> {
        match *slot {
            Some(id) => {
                self.replace_transient_root(id, value)?;
                Ok(id)
            }
            None => {
                let id = self.push_transient_root(value)?;
                *slot = Some(id);
                Ok(id)
            }
        }
    }

    fn indexed_iterate_kwarg_entry(
        &mut self,
        entry: TransientRootId,
        scratch: &mut SplatKeywordScratch,
    ) -> Result<SplatPreparation<(String, Value)>, VmError> {
        let entry_value = self.clone_transient_root(entry)?;
        let first_result =
            match self.splat_indexed_iterate_step(vec![entry_value, Value::I64(1)])? {
                SplatPreparation::Ready(result) => result,
                SplatPreparation::Raised => return Ok(SplatPreparation::Raised),
            };
        let key = self.julia_nth_field_checked(&first_result, 0)?;
        let state = self.julia_nth_field_checked(&first_result, 1)?;
        // Both values cross the second indexed-iterate call. Reuse fixed
        // scratch slots so a long keyword stream retains O(1) intermediates.
        let key = self.put_transient_root_slot(&mut scratch.key, key)?;
        let state = self.put_transient_root_slot(&mut scratch.state, state)?;

        let second_args = vec![
            self.clone_transient_root(entry)?,
            Value::I64(2),
            self.clone_transient_root(state)?,
        ];
        let second_result = match self.splat_indexed_iterate_step(second_args)? {
            SplatPreparation::Ready(result) => result,
            SplatPreparation::Raised => return Ok(SplatPreparation::Raised),
        };
        let value = self.julia_nth_field_checked(&second_result, 0)?;
        // Upstream obtains the value before asserting that the key is a Symbol.
        // Re-read the rooted key after the second call because GC may have
        // compacted a heap-backed key carrier during that call.
        let key_value = self.clone_transient_root(key)?;
        let key = match key_value {
            Value::Symbol(key) => key.as_str().to_string(),
            other => {
                return Err(VmError::TypeError(format!(
                    "in typeassert, expected Symbol, got a value of type {}",
                    self.get_type_name(&other)
                )))
            }
        };
        Ok(SplatPreparation::Ready((key, value)))
    }

    fn upsert_kwarg_root(
        &mut self,
        key: String,
        value: Value,
        kwargs: &mut KwargsMap<TransientRootId>,
    ) -> Result<(), VmError> {
        if let Some(existing) = kwargs.get(&key).copied() {
            self.replace_transient_root(existing, value)?;
        } else {
            let value = self.push_transient_root(value)?;
            kwargs.insert(key, value);
        }
        Ok(())
    }

    /// Snapshot the ordered keyword accumulator built so far as a `NamedTuple`
    /// `Value`, for real `merge(::NamedTuple, source)` dispatch (Issue #11381).
    fn kwargs_as_named_tuple_value(
        &mut self,
        kwargs: &KwargsMap<TransientRootId>,
    ) -> Result<Value, VmError> {
        let mut names = Vec::with_capacity(kwargs.len());
        let mut values = Vec::with_capacity(kwargs.len());
        for (name, &root) in kwargs.iter() {
            names.push(name.clone());
            values.push(self.clone_transient_root(root)?);
        }
        Ok(Value::NamedTuple(NamedTupleValue { names, values }))
    }

    /// Replace the ordered keyword accumulator wholesale with `names`/`values`
    /// (in that order), used when real `merge` dispatch (Issue #11381) already
    /// produced the complete merged `NamedTuple`/`Pairs` result rather than a
    /// source that still needs iterate-based merging.
    fn replace_kwargs_wholesale(
        &mut self,
        kwargs: &mut KwargsMap<TransientRootId>,
        names: Vec<String>,
        values: Vec<Value>,
    ) -> Result<(), VmError> {
        *kwargs = KwargsMap::new();
        for (name, value) in names.into_iter().zip(values) {
            self.upsert_kwarg_root(name, value, kwargs)?;
        }
        Ok(())
    }

    fn merge_streaming_kwarg_splat_source(
        &mut self,
        source: TransientRootId,
        kwargs: &mut KwargsMap<TransientRootId>,
    ) -> Result<SplatPreparation<()>, VmError> {
        // Unlike positional `_apply_iterate`, generic keyword merge always
        // streams the outer source through `iterate` one step at a time.
        let mut cursor = SplatValueCursor::Generic {
            collection: source,
            state: None,
        };
        let mut scratch = SplatKeywordScratch::default();
        loop {
            let entry = match self.next_generic_splat_value(&mut cursor)? {
                SplatPreparation::Ready(Some(entry)) => entry,
                SplatPreparation::Ready(None) => return Ok(SplatPreparation::Ready(())),
                SplatPreparation::Raised => return Ok(SplatPreparation::Raised),
            };
            let entry = self.put_transient_root_slot(&mut scratch.entry, entry)?;
            let (key, value) = match self.indexed_iterate_kwarg_entry(entry, &mut scratch)? {
                SplatPreparation::Ready(fields) => fields,
                SplatPreparation::Raised => return Ok(SplatPreparation::Raised),
            };
            self.upsert_kwarg_root(key, value, kwargs)?;
        }
    }

    /// Expand positional splats completely before target dispatch, retaining
    /// only remappable root handles across iterator calls.
    pub(in crate::vm) fn prepare_splat_argument_roots(
        &mut self,
        args: &[TransientRootId],
        splat_mask: &[bool],
    ) -> Result<SplatPreparation<Vec<TransientRootId>>, VmError> {
        let mut expanded = Vec::new();
        for (idx, &arg) in args.iter().enumerate() {
            if splat_mask.get(idx).copied().unwrap_or(false) {
                match self.collect_positional_splat_root(arg)? {
                    SplatPreparation::Ready(values) => expanded.extend(values),
                    SplatPreparation::Raised => return Ok(SplatPreparation::Raised),
                }
            } else {
                expanded.push(arg);
            }
        }
        Ok(SplatPreparation::Ready(expanded))
    }

    /// Try real `merge(::NamedTuple, source)` dispatch for a keyword-splat
    /// source that is not already a `NamedTuple`/`Pairs` fast path (Issue
    /// #11381). Resolution is purely structural — `find_best_method_index`
    /// matches on the VM's own type lattice, the same mechanism every other
    /// multi-method call site uses, so a user-defined `Base.merge(a::NamedTuple,
    /// ::T)` extension or a Base specialization (e.g.
    /// `merge(a::NamedTuple, b::Iterators.Zip{<:Tuple{Any,Any}})`'s
    /// duplicate-key validation) is found the same way any other method call
    /// would find it. Returns `None` when no candidate method applies at all,
    /// so the caller can fall back to the structural `iterate`-based merge
    /// unchanged.
    fn kwarg_merge_dispatch_step(
        &mut self,
        current_nt: Value,
        source_value: Value,
    ) -> Option<Result<SplatPreparation<Value>, VmError>> {
        self.find_best_method_index(&["merge"], &[current_nt.clone(), source_value.clone()])?;
        Some(self.sync_splat_callable_step("merge", vec![current_nt, source_value]))
    }

    fn merge_kwarg_splat_source(
        &mut self,
        source: TransientRootId,
        kwargs: &mut KwargsMap<TransientRootId>,
    ) -> Result<SplatPreparation<()>, VmError> {
        let source_value = self.clone_transient_root(source)?;
        match source_value {
            Value::NamedTuple(named_tuple) => {
                for (name, value) in named_tuple.names.into_iter().zip(named_tuple.values) {
                    self.upsert_kwarg_root(name, value, kwargs)?;
                }
                return Ok(SplatPreparation::Ready(()));
            }
            Value::Pairs(pairs) => {
                for (name, value) in pairs.data.names.into_iter().zip(pairs.data.values) {
                    self.upsert_kwarg_root(name, value, kwargs)?;
                }
                return Ok(SplatPreparation::Ready(()));
            }
            _ => {}
        }

        // Every other keyword-splat source is routed through real `merge`
        // dispatch (Issue #11381) before falling back to the structural
        // `iterate`-based merge. Never recognizes a package or type name in
        // Rust: `kwarg_merge_dispatch_step` returns `None` whenever no method
        // named `merge` applies to `(NamedTuple, source)` at all.
        let current_nt = self.kwargs_as_named_tuple_value(kwargs)?;
        let source_for_dispatch = self.clone_transient_root(source)?;
        if let Some(step) = self.kwarg_merge_dispatch_step(current_nt, source_for_dispatch) {
            return match step? {
                SplatPreparation::Ready(Value::NamedTuple(nt)) => {
                    self.replace_kwargs_wholesale(kwargs, nt.names, nt.values)?;
                    Ok(SplatPreparation::Ready(()))
                }
                SplatPreparation::Ready(Value::Pairs(pairs)) => {
                    self.replace_kwargs_wholesale(kwargs, pairs.data.names, pairs.data.values)?;
                    Ok(SplatPreparation::Ready(()))
                }
                SplatPreparation::Ready(other) => {
                    // `merge` validated and/or transformed the source further
                    // (e.g. Base's Zip specialization rejects duplicate keys,
                    // then hands the now-known-duplicate-free source back)
                    // without needing a runtime-parametric
                    // `NamedTuple{names}(values)` constructor. Fold the result
                    // into the *existing* (untouched) accumulator via the same
                    // structural iterate-based merge used for any other splat
                    // source — equivalent to upstream's `merge(a,
                    // NamedTuple(...))`, since `current_nt` above already
                    // captured the pre-merge accumulator unchanged.
                    let other_root = self.push_transient_root(other)?;
                    self.merge_streaming_kwarg_splat_source(other_root, kwargs)
                }
                SplatPreparation::Raised => Ok(SplatPreparation::Raised),
            };
        }

        self.merge_streaming_kwarg_splat_source(source, kwargs)
    }

    pub(in crate::vm) fn prepare_kwarg_value_roots(
        &mut self,
        names: &[String],
        splat_mask: &[bool],
        values: &[TransientRootId],
    ) -> Result<SplatPreparation<KwargsMap<TransientRootId>>, VmError> {
        // Insertion-ordered accumulator (Issue #11383): a duplicate name
        // replaces the value at its existing (first-occurrence) slot instead
        // of losing order to `HashMap`'s unspecified iteration.
        let mut kwargs = KwargsMap::with_capacity(names.len());
        for (idx, (name, &value)) in names.iter().zip(values).enumerate() {
            if splat_mask.get(idx).copied().unwrap_or(false) {
                match self.merge_kwarg_splat_source(value, &mut kwargs)? {
                    SplatPreparation::Ready(()) => {}
                    SplatPreparation::Raised => return Ok(SplatPreparation::Raised),
                }
            } else if let Some(existing) = kwargs.get(name).copied() {
                let value = self.clone_transient_root(value)?;
                self.replace_transient_root(existing, value)?;
            } else {
                kwargs.insert(name.clone(), value);
            }
        }
        Ok(SplatPreparation::Ready(kwargs))
    }

    /// Convenience boundary for non-opcode tests. Opcode implementations keep
    /// the surrounding transient frame alive through target dispatch.
    #[cfg(test)]
    pub(in crate::vm) fn prepare_splat_arguments(
        &mut self,
        args: Vec<Value>,
        splat_mask: &[bool],
    ) -> Result<SplatPreparation<Vec<Value>>, VmError> {
        self.with_transient_root_frame(|vm| {
            let args = args
                .into_iter()
                .map(|value| vm.push_transient_root(value))
                .collect::<Result<Vec<_>, _>>()?;
            match vm.prepare_splat_argument_roots(&args, splat_mask)? {
                SplatPreparation::Ready(values) => {
                    Ok(SplatPreparation::Ready(vm.clone_transient_roots(&values)?))
                }
                SplatPreparation::Raised => Ok(SplatPreparation::Raised),
            }
        })
    }

    #[cfg(test)]
    pub(in crate::vm) fn prepare_kwarg_values(
        &mut self,
        names: &[String],
        splat_mask: &[bool],
        values: Vec<Value>,
    ) -> Result<SplatPreparation<KwargsMap<Value>>, VmError> {
        self.with_transient_root_frame(|vm| {
            let values = values
                .into_iter()
                .map(|value| vm.push_transient_root(value))
                .collect::<Result<Vec<_>, _>>()?;
            match vm.prepare_kwarg_value_roots(names, splat_mask, &values)? {
                SplatPreparation::Ready(values) => {
                    let values = values
                        .into_iter()
                        .map(|(name, value)| Ok((name, vm.clone_transient_root(value)?)))
                        .collect::<Result<KwargsMap<_>, VmError>>()?;
                    Ok(SplatPreparation::Ready(values))
                }
                SplatPreparation::Raised => Ok(SplatPreparation::Raised),
            }
        })
    }

    /// Run one `iterate(...)` call synchronously, driving any started frame to
    /// completion (Issue #9127). Shared by
    /// [`Self::collect_iterator_via_iterate_protocol`].
    fn invoke_iterate_protocol_step(
        &mut self,
        iterate_fn: &Value,
        args: Vec<Value>,
    ) -> Result<Value, VmError> {
        use crate::vm::hof_exec::state::RuntimeCallableResult;
        let depth = self.frames.len();
        match self.call_runtime_callable_value(iterate_fn.clone(), args)? {
            RuntimeCallableResult::Immediate(value) => Ok(value),
            RuntimeCallableResult::StartedFrame => self.run_until_frame_return(depth),
            RuntimeCallableResult::Raised => Err(self.pending_error.take().unwrap_or_else(|| {
                VmError::InternalError("iterate raised during synchronous collect".to_string())
            })),
        }
    }

    fn collect_generator_iterate_protocol(
        &mut self,
        generator: &GeneratorValue,
    ) -> Result<Value, VmError> {
        let mut values = Vec::new();
        let mut state: Option<Value> = None;
        loop {
            let next = self.invoke_generator_iterate_step(generator, state.as_ref())?;
            match next {
                Value::Nothing => break,
                Value::Tuple(tuple) if tuple.elements.len() == 2 => {
                    let mut elements = tuple.elements.into_iter();
                    let element = elements.next().ok_or_else(|| {
                        VmError::TypeError("Generator iterate result missing element".to_string())
                    })?;
                    let next_state = elements.next().ok_or_else(|| {
                        VmError::TypeError("Generator iterate result missing state".to_string())
                    })?;
                    values.push(element);
                    state = Some(next_state);
                }
                other => {
                    return Err(VmError::TypeError(format!(
                        "collect: generator iterate must return `nothing` or a 2-tuple, got {:?}",
                        other.runtime_type()
                    )));
                }
            }
        }
        let element_type = generator
            .result_element_type
            .clone()
            .unwrap_or(ArrayElementType::Any);
        let arr = ArrayValue::memory_first_collect_typejoin_values(values, element_type)?;
        self.array_wrapper_value(arr)
    }

    fn invoke_generator_iterate_step(
        &mut self,
        generator: &GeneratorValue,
        state: Option<&Value>,
    ) -> Result<Value, VmError> {
        let frame_depth = self.frames.len();
        let stack_depth = self.stack.len();
        if !self.start_lazy_generator_iterate_call(generator, state)? {
            return if let Some(state) = state {
                self.iterate_next(generator.iter.as_ref(), state)
            } else {
                self.iterate_first(generator.iter.as_ref())
            };
        }
        if self.frames.len() > frame_depth {
            return self.run_until_frame_return(frame_depth);
        }
        if self.stack.len() > stack_depth {
            return self.stack.pop_value();
        }
        Err(self.pending_error.take().unwrap_or_else(|| {
            VmError::InternalError(
                "generator iterate did not produce a synchronous result".to_string(),
            )
        }))
    }

    /// Eagerly materialize a lazy generator's base iterator when it can only be
    /// iterated through a pure-Julia `iterate` method (Issue #9127).
    ///
    /// Every lazy-generator consumer (`collect`/`sum`/`first`/`count`/`for`)
    /// drives the base iterator through the synchronous Rust
    /// `iterate_first`/`iterate_next` fast paths, which cover native arrays,
    /// ranges, tuples, `Memory`, the `Array{T,N}` wrapper, and the builtin
    /// iterator structs (`Zip` / `Enumerate` / `CartesianIndices` / …) but NOT
    /// `Dict` / `KeySet` / `ValueIterator` or user-defined iterables, whose
    /// `iterate` lives in pure Julia and needs interpreter re-entry.
    ///
    /// Rather than teach every consumer to re-enter, materialize such a base
    /// ONCE at generator construction into a plain array of its elements. The
    /// generator's body mapping stays lazy — only the finite, side-effect-free
    /// base traversal becomes eager — so laziness of the mapped side effects is
    /// preserved while `collect` / `first` / `count` / `sum` all work.
    pub(in crate::vm) fn materialize_generator_base_if_needed(
        &mut self,
        iter: Value,
    ) -> Result<Value, VmError> {
        // Hot path: native carriers and the public `Array{T,N}` wrapper iterate
        // natively — keep them lazy without probing.
        if is_native_array_value(&iter) || self.is_array_wrapper_iter(&iter) {
            return Ok(iter);
        }
        // Only struct-backed iterables can require a pure-Julia `iterate`; Range /
        // Tuple / Memory / Str / SimpleVector / StaticArray are builtin-iterable.
        if !matches!(iter, Value::Struct(_) | Value::StructRef(_)) {
            return Ok(iter);
        }
        // Issue #9200 (S4a): a `Product` / `ProductIterator` base (the desugared
        // comma product generator's iterator) must materialize with its N-D shape
        // preserved so the generator's `collect` yields a Matrix, not a flat
        // Vector. Do this BEFORE the `iterate_first` probe — even if the Rust fast
        // path could step the product lazily, the eager shape-preserving
        // materialization here is what keeps `collect` shaped (its base traversal
        // is side-effect-free, so body laziness is unaffected).
        if let Some((struct_name, fields)) = self.product_struct_name_and_fields(&iter) {
            return self.collect_product_iterator(&iter, &struct_name, &fields);
        }
        // The builtin iterator structs (Zip / Enumerate / CartesianIndices / …)
        // are served by the Rust fast path; only materialize when it cannot even
        // start the iteration (Dict / KeySet / ValueIterator / user iterables).
        if self.iterate_first(&iter).is_ok() {
            return Ok(iter);
        }
        if self
            .find_best_method_index(&["iterate"], std::slice::from_ref(&iter))
            .is_some()
        {
            return self.collect_iterator_via_iterate_protocol(&iter);
        }
        // No applicable `iterate` method: leave the base untouched so the normal
        // "no method matching iterate" / "unsupported iterator type" error still
        // surfaces from the consumer.
        Ok(iter)
    }

    fn is_array_wrapper_iter(&self, iter: &Value) -> bool {
        match iter {
            Value::Struct(s) => Self::is_array_wrapper_struct_name(&s.struct_name),
            Value::StructRef(idx) => self
                .struct_heap
                .get(*idx)
                .is_some_and(|s| Self::is_array_wrapper_struct_name(&s.struct_name)),
            _ => false,
        }
    }

    /// The `(struct_name, fields)` of a `Product` / `ProductIterator` value, or
    /// `None` for any other value (Issue #9200 S4a).
    fn product_struct_name_and_fields(&self, iter: &Value) -> Option<(String, Vec<Value>)> {
        match iter {
            Value::Struct(s) if Self::is_product_iterator_struct_name(&s.struct_name) => {
                Some((s.struct_name.to_string(), s.values.clone()))
            }
            Value::StructRef(idx) => self
                .struct_heap
                .get(*idx)
                .filter(|s| Self::is_product_iterator_struct_name(&s.struct_name))
                .map(|s| (s.struct_name.to_string(), s.values.clone())),
            _ => None,
        }
    }

    fn collect_zip_fields(&mut self, fields: &[Value]) -> Result<Value, VmError> {
        if fields.is_empty() {
            let arr = ArrayValue::memory_first_collect_values(Vec::new(), ArrayElementType::Any)?;
            return self.array_wrapper_value(arr);
        }

        let mut values = Vec::with_capacity(fields.len());
        let mut states = Vec::with_capacity(fields.len());
        for field in fields {
            match self.iterate_first(field)? {
                Value::Tuple(tuple) if tuple.elements.len() == 2 => {
                    values.push(tuple.elements[0].clone());
                    states.push(tuple.elements[1].clone());
                }
                Value::Nothing => {
                    let arr =
                        ArrayValue::memory_first_collect_values(Vec::new(), ArrayElementType::Any)?;
                    return self.array_wrapper_value(arr);
                }
                _ => {
                    return Err(VmError::TypeError(
                        "collect(zip): iterate result must be Tuple or Nothing".to_string(),
                    ));
                }
            }
        }

        let mut zipped = Vec::new();
        loop {
            zipped.push(Value::Tuple(TupleValue {
                elements: values.clone(),
            }));

            let mut next_values = Vec::with_capacity(fields.len());
            let mut next_states = Vec::with_capacity(fields.len());
            for (field, state) in fields.iter().zip(states.iter()) {
                match self.iterate_next(field, state)? {
                    Value::Tuple(tuple) if tuple.elements.len() == 2 => {
                        next_values.push(tuple.elements[0].clone());
                        next_states.push(tuple.elements[1].clone());
                    }
                    Value::Nothing => {
                        let arr =
                            ArrayValue::memory_first_collect_values(zipped, ArrayElementType::Any)?;
                        return self.array_wrapper_value(arr);
                    }
                    _ => {
                        return Err(VmError::TypeError(
                            "collect(zip): iterate result must be Tuple or Nothing".to_string(),
                        ));
                    }
                }
            }
            values = next_values;
            states = next_states;
        }
    }

    fn collect_enumerate_fields(&mut self, fields: &[Value]) -> Result<Value, VmError> {
        let Some(iter) = fields.first() else {
            return Err(VmError::TypeError(
                "collect(enumerate): missing wrapped iterator".to_string(),
            ));
        };
        let (values, _) = self.collect_iterator_values(iter)?;
        let enumerated = values
            .into_iter()
            .enumerate()
            .map(|(idx, value)| {
                Value::Tuple(TupleValue {
                    elements: vec![Value::I64(idx as i64 + 1), value],
                })
            })
            .collect();
        let arr = ArrayValue::memory_first_collect_values(enumerated, ArrayElementType::Any)?;
        self.array_wrapper_value(arr)
    }

    fn collect_rest_fields(&mut self, fields: &[Value]) -> Result<Value, VmError> {
        let (Some(iter), Some(initial_state)) = (fields.first(), fields.get(1)) else {
            return Err(VmError::TypeError(
                "collect(rest): missing iterator or state".to_string(),
            ));
        };

        let mut values = Vec::new();
        let mut next = self.iterate_next(iter, initial_state)?;
        loop {
            match next {
                Value::Tuple(ref tuple) if tuple.elements.len() == 2 => {
                    values.push(tuple.elements[0].clone());
                    next = self.iterate_next(iter, &tuple.elements[1])?;
                }
                Value::Nothing => break,
                _ => {
                    return Err(VmError::TypeError(
                        "collect(rest): iterate result must be Tuple or Nothing".to_string(),
                    ));
                }
            }
        }

        let arr = ArrayValue::memory_first_collect_values(values, ArrayElementType::Any)?;
        self.array_wrapper_value(arr)
    }

    fn collect_logrange_fields(&mut self, fields: &[Value]) -> Result<Value, VmError> {
        if fields.len() < 5 {
            return Err(VmError::TypeError(
                "collect(LogRange): invalid struct fields".to_string(),
            ));
        }
        let start = match &fields[FIRST_FIELD_INDEX] {
            Value::F64(f) => *f,
            Value::I64(i) => *i as f64,
            _ => {
                return Err(VmError::TypeError(
                    "LogRange: start must be numeric".to_string(),
                ))
            }
        };
        let stop = match &fields[SECOND_FIELD_INDEX] {
            Value::F64(f) => *f,
            Value::I64(i) => *i as f64,
            _ => {
                return Err(VmError::TypeError(
                    "LogRange: stop must be numeric".to_string(),
                ))
            }
        };
        let len = match &fields[THIRD_FIELD_INDEX] {
            Value::I64(n) => *n,
            _ => return Err(VmError::TypeError("LogRange: len must be I64".to_string())),
        };
        if len <= 0 {
            return self
                .array_wrapper_value(ArrayValue::memory_first_from_f64(Vec::new(), vec![0]));
        }
        let log_start_div = match &fields[FOURTH_FIELD_INDEX] {
            Value::F64(f) => *f,
            Value::I64(i) => *i as f64,
            _ => {
                return Err(VmError::TypeError(
                    "LogRange: log_start_div must be numeric".to_string(),
                ))
            }
        };
        let log_stop_div = match &fields[4] {
            Value::F64(f) => *f,
            Value::I64(i) => *i as f64,
            _ => {
                return Err(VmError::TypeError(
                    "LogRange: log_stop_div must be numeric".to_string(),
                ))
            }
        };

        let mut values = Vec::with_capacity(len as usize);
        for idx in 1..=len {
            let value = if idx == 1 {
                start
            } else if idx == len {
                stop
            } else {
                let i = idx as f64;
                let len_f = len as f64;
                ((len_f - i) * log_start_div + (i - 1.0) * log_stop_div).exp()
            };
            values.push(value);
        }
        self.array_wrapper_value(ArrayValue::memory_first_from_f64(
            values,
            vec![len as usize],
        ))
    }

    fn generator_empty_element_type(
        &self,
        func_index: usize,
        result_element_type: Option<ArrayElementType>,
    ) -> ArrayElementType {
        result_element_type.unwrap_or_else(|| {
            self.functions
                .get(func_index)
                .map(|func| ArrayElementType::from_value_type(&func.return_type))
                .unwrap_or(ArrayElementType::Any)
        })
    }

    fn function_name_is_identity(name: &str) -> bool {
        name == "identity" || name == "Base.identity"
    }

    fn callable_value_is_identity(value: &Value) -> bool {
        match value {
            Value::Function(function) => Self::function_name_is_identity(&function.name),
            Value::Closure(closure) => Self::function_name_is_identity(&closure.name),
            _ => false,
        }
    }

    fn generator_callable_is_identity_map(&self, callable: &GeneratorCallable) -> bool {
        match callable {
            GeneratorCallable::FunctionIndex(func_index) => self
                .functions
                .get(*func_index)
                .is_some_and(|function| Self::function_name_is_identity(&function.name)),
            GeneratorCallable::FilteredFunctionIndex { map_func_index, .. } => self
                .functions
                .get(*map_func_index)
                .is_some_and(|function| Self::function_name_is_identity(&function.name)),
            GeneratorCallable::RuntimeValue(value) => Self::callable_value_is_identity(value),
            GeneratorCallable::FilteredRuntimeValue { map, .. } => {
                Self::callable_value_is_identity(map)
            }
            _ => false,
        }
    }

    fn empty_sum_zero_value(element_type: &ArrayElementType) -> Option<Value> {
        match element_type {
            ArrayElementType::Bool
            | ArrayElementType::I8
            | ArrayElementType::I16
            | ArrayElementType::I32
            | ArrayElementType::I64 => Some(Value::I64(0)),
            ArrayElementType::I128 => Some(Value::I128(0)),
            ArrayElementType::U8
            | ArrayElementType::U16
            | ArrayElementType::U32
            | ArrayElementType::U64 => Some(Value::U64(0)),
            ArrayElementType::U128 => Some(Value::U128(0)),
            ArrayElementType::F16 => Some(Value::F16(f16::from_f32(0.0))),
            ArrayElementType::F32 => Some(Value::F32(0.0)),
            ArrayElementType::F64 => Some(Value::F64(0.0)),
            _ => None,
        }
    }

    pub(in crate::vm) fn generator_empty_sum_value(
        &mut self,
        generator: &GeneratorValue,
    ) -> Result<Option<Value>, VmError> {
        let empty_reduce_message =
            "reducing over an empty collection is not allowed; consider supplying `init` to the reducer";
        if !self.generator_callable_is_identity_map(&generator.callable) {
            self.raise(VmError::ArgumentError(empty_reduce_message.to_string()))?;
            return Ok(None);
        }
        let Some(element_type) = generator.result_element_type.as_ref() else {
            self.raise(VmError::ArgumentError(empty_reduce_message.to_string()))?;
            return Ok(None);
        };
        match Self::empty_sum_zero_value(element_type) {
            Some(value) => Ok(Some(value)),
            None => {
                self.raise(VmError::ArgumentError(empty_reduce_message.to_string()))?;
                Ok(None)
            }
        }
    }

    fn tuple_splat_type_object_empty_element_type(
        &self,
        jt: &JuliaType,
        iter: &Value,
        result_element_type: Option<ArrayElementType>,
    ) -> ArrayElementType {
        result_element_type.unwrap_or_else(|| {
            let arg_count = self
                .runtime_generator_arg_types(iter, true)
                .map(|arg_types| arg_types.len())
                .unwrap_or(0);
            let type_name = jt.name();
            let has_matching_struct_constructor = self.struct_defs.iter().any(|def| {
                def.fields.len() == arg_count
                    && (def.name == type_name
                        || type_name
                            .split_once('{')
                            .is_some_and(|(base, _)| def.name == base))
            });
            if has_matching_struct_constructor {
                array_element_type_from_julia_type(jt)
            } else {
                ArrayElementType::UnionOf(Vec::new())
            }
        })
    }

    fn collect_iterator_values(
        &mut self,
        iter: &Value,
    ) -> Result<(Vec<Value>, Vec<usize>), VmError> {
        let collected = self.collect_iterator(iter)?;
        let arr = if let Some(arr_ref) = native_array_value_ref(&collected) {
            arr_ref.borrow().clone()
        } else if let Some(arr) = array_wrapper_value_to_array_value(&collected, &self.struct_heap)?
        {
            arr
        } else {
            return Err(VmError::TypeError(
                "collect: iterator materialization did not produce an Array".to_string(),
            ));
        };

        let mut values = Vec::with_capacity(arr.element_count());
        for idx in 0..arr.element_count() {
            values.push(arr.get_linear(idx)?);
        }
        Ok((values, arr.shape.clone()))
    }

    pub(in crate::vm) fn generator_projected_field(
        &self,
        generator: &GeneratorValue,
        field_name: &str,
    ) -> Result<Value, VmError> {
        match field_name {
            "iter" => Ok(generator.iter.as_ref().clone()),
            "f" => self.generator_callable_field_value(&generator.callable),
            // Issue #10212: nonexistent field -> FieldError, matching upstream 1.12.
            _ => Err(VmError::FieldError {
                type_name: "Base.Generator".to_string(),
                field: field_name.to_string(),
            }),
        }
    }

    /// Project one of `Base.Generator`'s two upstream physical fields
    /// (`f`, `iter`) by 0-based positional index. Returns `Ok(None)` for any
    /// other index so the caller's shared `FieldIndexOutOfBounds`/
    /// `BoundsError` handling applies — mirroring
    /// `RegexMatchValue::field_by_index` (Issue #11382) — instead of raising
    /// here with no receiver and the wrong (0-based) index (Issue #11509).
    pub(in crate::vm) fn generator_projected_field_by_index(
        &self,
        generator: &GeneratorValue,
        field_idx: usize,
    ) -> Result<Option<Value>, VmError> {
        match field_idx {
            0 => self
                .generator_callable_field_value(&generator.callable)
                .map(Some),
            1 => Ok(Some(generator.iter.as_ref().clone())),
            _ => Ok(None),
        }
    }

    pub(in crate::vm) fn generator_callable_field_value(
        &self,
        callable: &GeneratorCallable,
    ) -> Result<Value, VmError> {
        match callable {
            GeneratorCallable::FunctionIndex(func_index) => {
                let function = self.functions.get(*func_index).ok_or_else(|| {
                    VmError::TypeError(format!(
                        "Base.Generator callable references invalid function index {func_index}"
                    ))
                })?;
                Ok(Value::Function(self.function_value_with_candidates(
                    function.name.clone(),
                    vec![*func_index],
                )))
            }
            GeneratorCallable::TypeObject(julia_type) => {
                Ok(Value::DataType(Box::new(julia_type.clone())))
            }
            GeneratorCallable::RuntimeValue(value) => Ok(value.as_ref().clone()),
            GeneratorCallable::TupleSplatFunctionIndex(_)
            | GeneratorCallable::TupleSplatTypeObject(_)
            | GeneratorCallable::TupleSplatRuntimeValue(_)
            | GeneratorCallable::FilteredFunctionIndex { .. }
            | GeneratorCallable::FilteredRuntimeValue { .. }
            | GeneratorCallable::Eager => Err(VmError::TypeError(
                "Base.Generator callable field is not representable as a direct callable value"
                    .to_string(),
            )),
        }
    }

    /// Fuse a chain of nested LAZY generators into a single mapping step over
    /// the base iterator (Issue #9103): `(callable, Generator(g, it))` becomes
    /// `(callable ∘ g, it)`, repeatedly, so the synchronous
    /// `collect_iterator_values` boundary only ever sees the base iterator.
    ///
    /// Fusion applies only when both the outer and the inner callable are
    /// plain single-argument callables representable as runtime values
    /// (`FunctionIndex` / `TypeObject` / `RuntimeValue`). Tuple-splat,
    /// filtered, and eager shapes keep their original (callable, iter) pair —
    /// the eager inner generator already materialized its values, and the
    /// splat/filtered shapes have argument conventions plain `∘` composition
    /// would break.
    pub(in crate::vm) fn fuse_lazy_generator_iter(
        &self,
        callable: GeneratorCallable,
        iter: &Value,
    ) -> (GeneratorCallable, Value) {
        let mut callable = callable;
        let mut iter = iter.clone();
        loop {
            let Value::Generator(inner) = &iter else {
                return (callable, iter);
            };
            if !matches!(
                callable,
                GeneratorCallable::FunctionIndex(_)
                    | GeneratorCallable::TypeObject(_)
                    | GeneratorCallable::RuntimeValue(_)
            ) || !matches!(
                inner.callable,
                GeneratorCallable::FunctionIndex(_)
                    | GeneratorCallable::TypeObject(_)
                    | GeneratorCallable::RuntimeValue(_)
            ) {
                return (callable, iter);
            }
            let (Ok(outer_value), Ok(inner_value)) = (
                self.generator_callable_field_value(&callable),
                self.generator_callable_field_value(&inner.callable),
            ) else {
                return (callable, iter);
            };
            callable = GeneratorCallable::RuntimeValue(Box::new(Value::ComposedFunction(
                crate::vm::value::ComposedFunctionValue::new(outer_value, inner_value),
            )));
            let next_iter = inner.iter.as_ref().clone();
            iter = next_iter;
        }
    }

    /// Collect Generator by applying function to each element.
    ///
    /// Mirrors Julia's `iterate(g::Generator)` in `julia/base/generator.jl` by
    /// applying `g.f` to values from `g.iter`. The VM iterator protocol itself
    /// is still synchronous, so lazy Generator collection materializes the
    /// wrapped iterator first and then uses the existing value-mode HOF path to
    /// call `f` for each value. Empty lazy generators use the compile-time
    /// `@default_eltype` equivalent when available, matching
    /// `julia/base/array.jl`'s empty `collect(itr::Generator)` branch.
    ///
    /// Eager generators already hold the result array. Function-index
    /// generators enter the existing HOF frame path. Type-object generators
    /// are applied synchronously, matching Julia's callable DataType behavior.
    pub(in crate::vm) fn collect_generator(
        &mut self,
        callable: GeneratorCallable,
        iter: &Value,
        result_element_type: Option<ArrayElementType>,
    ) -> Result<Option<Value>, VmError> {
        // Generator fusion (Issue #9103): when the wrapped iterator is itself
        // a LAZY generator (`Generator(f, Generator(g, it))` — e.g.
        // `map(f, (g(x) for x in it))`), the synchronous
        // `collect_iterator_values` below cannot materialize it (function
        // callables need frame re-entry). Fuse the two mapping steps into one
        // composed callable over the base iterator — `Generator(f ∘ g, it)` —
        // which the runtime-callable HOF path already knows how to drive.
        let (callable, fused_iter) = self.fuse_lazy_generator_iter(callable, iter);
        let iter = &fused_iter;
        match callable {
            GeneratorCallable::Eager => {
                // Eager-evaluated generator: iter is already the collected array
                // Just return a copy of it
                // CollectFallback: generator-eager-copy-boundary
                self.collect_iterator(iter).map(Some)
            }
            GeneratorCallable::TypeObject(jt) => {
                // CollectFallback: generator-typeobject-boundary
                let empty_element_type =
                    result_element_type.unwrap_or_else(|| array_element_type_from_julia_type(&jt));
                self.collect_generator(
                    GeneratorCallable::RuntimeValue(Box::new(Value::DataType(Box::new(jt)))),
                    iter,
                    Some(empty_element_type),
                )
            }
            GeneratorCallable::TupleSplatTypeObject(jt) => {
                // CollectFallback: generator-tuplesplat-typeobject-boundary
                let empty_element_type =
                    self.tuple_splat_type_object_empty_element_type(&jt, iter, result_element_type);
                self.collect_generator(
                    GeneratorCallable::TupleSplatRuntimeValue(Box::new(Value::DataType(Box::new(
                        jt,
                    )))),
                    iter,
                    Some(empty_element_type),
                )
            }
            GeneratorCallable::FunctionIndex(func_index) => {
                // CollectFallback: generator-function-index-boundary
                let (values, shape) = self.collect_iterator_values(iter)?;
                if values.is_empty() {
                    let element_type =
                        self.generator_empty_element_type(func_index, result_element_type);
                    let mut arr = ArrayValue::memory_first_with_capacity(element_type, 0);
                    arr.shape = shape;
                    return self.array_wrapper_value(arr).map(Some);
                }
                self.start_hof_call_values_with_array_result(
                    func_index,
                    values,
                    shape,
                    HofOpKind::Broadcast,
                    true,
                )?;
                Ok(None)
            }
            GeneratorCallable::FilteredFunctionIndex {
                map_func_index,
                predicate_func_index,
            } => {
                // CollectFallback: generator-filtered-function-boundary
                let (values, _shape) = self.collect_iterator_values(iter)?;
                if values.is_empty() {
                    let predicate_uses_inlined_call = self
                        .functions
                        .get(predicate_func_index)
                        .is_some_and(|func| {
                            func.slot_names
                                .iter()
                                .any(|name| name.starts_with("__sjulia_inline_arg_"))
                        });
                    let element_type = if predicate_uses_inlined_call {
                        ArrayElementType::UnionOf(Vec::new())
                    } else {
                        result_element_type.unwrap_or_else(|| ArrayElementType::UnionOf(Vec::new()))
                    };
                    let arr = ArrayValue::memory_first_with_capacity(element_type, 0);
                    return self.array_wrapper_value(arr).map(Some);
                }
                self.start_hof_filter_map_values_with_array_result(
                    predicate_func_index,
                    map_func_index,
                    values,
                    result_element_type,
                    true,
                )?;
                Ok(None)
            }
            GeneratorCallable::FilteredRuntimeValue { map, predicate } => {
                // Issue #9271: filtered generator whose lifted body/predicate are
                // runtime callables (function-scope / capturing). Materialize the
                // base iterator, then drive the alternating predicate → map HOF
                // over runtime callables so side effects fire lazily (at collect
                // time), matching upstream ordering.
                // CollectFallback: generator-filtered-runtime-callable-boundary
                let (values, _shape) = self.collect_iterator_values(iter)?;
                self.start_hof_filter_map_runtime_values_with_array_result(
                    predicate.as_ref().clone(),
                    map.as_ref().clone(),
                    values,
                    result_element_type,
                    true,
                )?;
                Ok(None)
            }
            GeneratorCallable::TupleSplatFunctionIndex(func_index) => {
                // CollectFallback: generator-tuplesplat-function-boundary
                let (values, shape) = self.collect_iterator_values(iter)?;
                if values.is_empty() {
                    let element_type =
                        self.generator_empty_element_type(func_index, result_element_type);
                    let mut arr = ArrayValue::memory_first_with_capacity(element_type, 0);
                    arr.shape = shape;
                    return self.array_wrapper_value(arr).map(Some);
                }
                self.start_hof_call_values_with_array_result(
                    func_index,
                    values,
                    shape,
                    HofOpKind::BroadcastTupleSplat,
                    true,
                )?;
                Ok(None)
            }
            GeneratorCallable::RuntimeValue(callable) => {
                // CollectFallback: generator-runtime-callable-boundary
                let (values, shape) = self.collect_iterator_values(iter)?;
                if values.is_empty() {
                    let element_type = result_element_type.unwrap_or(ArrayElementType::Any);
                    let mut arr = ArrayValue::memory_first_with_capacity(element_type, 0);
                    arr.shape = shape;
                    return self.array_wrapper_value(arr).map(Some);
                }
                self.start_hof_runtime_call_values_with_array_result(
                    callable.as_ref().clone(),
                    values,
                    shape,
                    HofOpKind::Broadcast,
                    true,
                )?;
                Ok(None)
            }
            GeneratorCallable::TupleSplatRuntimeValue(callable) => {
                // CollectFallback: generator-tuplesplat-runtime-callable-boundary
                let (values, shape) = self.collect_iterator_values(iter)?;
                if values.is_empty() {
                    let element_type = match callable.as_ref() {
                        Value::DataType(jt) => self.tuple_splat_type_object_empty_element_type(
                            jt.as_ref(),
                            iter,
                            result_element_type,
                        ),
                        _ => result_element_type.unwrap_or(ArrayElementType::Any),
                    };
                    let mut arr = ArrayValue::memory_first_with_capacity(element_type, 0);
                    arr.shape = shape;
                    return self.array_wrapper_value(arr).map(Some);
                }
                self.start_hof_runtime_call_values_with_array_result(
                    callable.as_ref().clone(),
                    values,
                    shape,
                    HofOpKind::BroadcastTupleSplat,
                    true,
                )?;
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod splat_preparation_tests {
    use super::*;
    use crate::rng::StableRng;
    use crate::types::JuliaType;
    use crate::vm::splat::SplatPreparation;
    use crate::vm::types::FunctionInfo;
    use crate::vm::value::{
        native_array_value_from_array, new_memory_ref, ArrayData, ArrayElementType, ArrayValue,
        MemoryValue, NamedTupleValue, RangeValue, StructInstance, TupleValue, Value, ValueType,
    };
    use crate::vm::{Instr, Vm};
    use std::rc::Rc;

    fn ready_values(result: Result<SplatPreparation<Vec<Value>>, VmError>) -> Vec<Value> {
        match result {
            Ok(SplatPreparation::Ready(values)) => values,
            Ok(SplatPreparation::Raised) => panic!("splat preparation unexpectedly raised"),
            Err(err) => panic!("splat preparation failed: {err}"),
        }
    }

    fn test_function(name: &str, param_types: Vec<JuliaType>) -> FunctionInfo {
        FunctionInfo {
            name: name.to_string(),
            params: param_types
                .iter()
                .enumerate()
                .map(|(idx, _)| (format!("x{idx}"), ValueType::Any))
                .collect(),
            kwparams: vec![],
            entry: 0,
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            is_generated: false,
            is_lowering_helper: false,
            definition_order: 0,
            min_world: 1,
            type_params: vec![],
            param_julia_types: param_types.clone(),
            code_start: 0,
            code_end: 2,
            slot_names: vec![],
            slot_types: vec![],
            local_slot_count: param_types.len(),
            param_slots: (0..param_types.len()).collect(),
            vararg_param_index: None,
            vararg_fixed_count: None,
            inlining_meta: 0,
            constprop_meta: 0,
            nospecialize_meta: 0,
            propagate_inbounds_meta: false,
            nospecializeinfer_meta: false,
            purity_meta: 0,
            direct_return_type_param: None,
            def_line: 0,
            suppress_short_name_alias: false,
            shared_plan: None,
        }
    }

    #[test]
    fn prepare_splat_respects_mask_and_number_iteration_11372() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let unchanged =
            ready_values(vm.prepare_splat_arguments(vec![Value::Nothing, Value::I64(7)], &[false]));
        assert!(matches!(
            unchanged.as_slice(),
            [Value::Nothing, Value::I64(7)]
        ));

        let singleton = ready_values(vm.prepare_splat_arguments(vec![Value::I64(42)], &[true]));
        assert!(matches!(singleton.as_slice(), [Value::I64(42)]));
    }

    #[test]
    fn prepare_splat_iterates_range_and_expands_array_11372() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let range = Value::Range(RangeValue::unit_range(3.0, 4.0));
        let array = native_array_value_from_array(ArrayValue::any_vector(vec![
            Value::I64(5),
            Value::I64(6),
        ]));

        let values = ready_values(vm.prepare_splat_arguments(vec![range, array], &[true, true]));
        assert!(matches!(
            values.as_slice(),
            [Value::I64(3), Value::I64(4), Value::I64(5), Value::I64(6)]
        ));
    }

    #[test]
    fn memory_iteration_uses_upstream_one_based_state_11389() {
        let vm = Vm::new(Vec::new(), StableRng::new(0));
        let memory = Value::Memory(new_memory_ref(MemoryValue::new(
            ArrayData::I64(vec![10, 20]),
            ArrayElementType::I64,
            2,
        )));

        assert!(matches!(
            vm.iterate_first_fast(&memory),
            Ok(Some(Some((Value::I64(10), Value::I64(2)))))
        ));
        assert!(matches!(
            vm.iterate_next_fast(&memory, &Value::I64(2)),
            Ok(Some(Some((Value::I64(20), Value::I64(3)))))
        ));
        assert!(matches!(
            vm.iterate_next_fast(&memory, &Value::I64(3)),
            Ok(Some(None))
        ));
        assert!(matches!(
            vm.iterate_first(&memory),
            Ok(Value::Tuple(ref step))
                if matches!(step.elements.as_slice(), [Value::I64(10), Value::I64(2)])
        ));
        assert!(matches!(
            vm.iterate_next(&memory, &Value::I64(2)),
            Ok(Value::Tuple(ref step))
                if matches!(step.elements.as_slice(), [Value::I64(20), Value::I64(3)])
        ));
    }

    #[test]
    fn prepare_splat_rejects_invalid_struct_ref_11372() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let error = match vm.prepare_splat_arguments(vec![Value::StructRef(99)], &[true]) {
            Err(error) => error,
            Ok(_) => panic!("invalid StructRef unexpectedly prepared"),
        };
        assert!(matches!(error, VmError::InternalError(_)));
    }

    #[test]
    fn splat_iteration_rechecks_dispatch_for_each_state_11372() {
        // No one-argument method is installed, so String's first step uses the
        // native fallback. The two-argument override then terminates before
        // native iteration can yield the second character.
        let code = vec![Instr::PushNothing, Instr::ReturnAny];
        let mut vm = Vm::new(code, StableRng::new(0));
        vm.functions.push(Rc::new(test_function(
            "iterate",
            vec![JuliaType::String, JuliaType::Int64],
        )));
        vm.function_name_index
            .insert("iterate".to_string(), vec![0]);

        let values = ready_values(vm.prepare_splat_arguments(vec![Value::str_new("ab")], &[true]));
        assert!(matches!(values.as_slice(), [Value::Char('a')]));
    }

    #[test]
    fn string_iteration_uses_upstream_codeunit_states_11372() {
        let bytes = "a😊b".as_bytes();
        assert!(matches!(
            string_iterate_at(bytes, 1),
            Some((Value::Char('a'), Value::I64(2)))
        ));
        assert!(matches!(
            string_iterate_at(bytes, 2),
            Some((Value::Char('😊'), Value::I64(6)))
        ));
        assert!(matches!(
            string_iterate_at(bytes, 6),
            Some((Value::Char('b'), Value::I64(7)))
        ));
        assert!(string_iterate_at(bytes, 7).is_none());

        assert!(matches!(
            string_iterate_at(&[0xFF, b'a'], 1),
            Some((Value::CharMalformed(0xFF00_0000), Value::I64(2)))
        ));
        assert!(matches!(
            string_iterate_at(&[0xFF, b'a'], 2),
            Some((Value::Char('a'), Value::I64(3)))
        ));
    }

    #[test]
    fn string_splat_honors_codeunit_state_across_method_fallback_11372() {
        // A user one-argument iterate method returns upstream's byte-index state
        // for "éa". With no matching two-argument method, the native fallback
        // must resume at byte 3 and yield `a`, not interpret 3 as a character
        // ordinal and stop.
        let code = vec![
            Instr::PushChar('é'),
            Instr::PushI64(3),
            Instr::NewTuple(2),
            Instr::ReturnAny,
        ];
        let mut vm = Vm::new(code, StableRng::new(0));
        let mut first = test_function("iterate", vec![JuliaType::String]);
        first.code_end = 4;
        vm.functions.push(Rc::new(first));
        vm.function_name_index
            .insert("iterate".to_string(), vec![0]);

        let values = ready_values(vm.prepare_splat_arguments(vec![Value::str_new("éa")], &[true]));
        assert!(matches!(
            values.as_slice(),
            [Value::Char('é'), Value::Char('a')]
        ));
    }

    #[test]
    fn range_iteration_uses_upstream_state_kinds_11387() {
        let mut ordinal = RangeValue::unit_range(3.0, 5.0);
        ordinal.element_type = RangeElementType::Int8;
        assert!(matches!(
            first_range_iteration(&ordinal),
            Ok(Some((Value::I8(3), Value::I8(3))))
        ));
        assert!(matches!(
            next_range_iteration(&ordinal, &Value::I8(3)),
            Ok(Some((Value::I8(4), Value::I8(4))))
        ));
        assert!(matches!(
            next_range_iteration(&ordinal, &Value::I8(5)),
            Ok(None)
        ));

        let step_range_len = RangeValue::float_linspace(1.0, 2.0, 3, RangeElementType::Float64);
        assert!(matches!(
            first_range_iteration(&step_range_len),
            Ok(Some((Value::F64(value), Value::I64(1)))) if value == 1.0
        ));
        assert!(matches!(
            next_range_iteration(&step_range_len, &Value::I64(1)),
            Ok(Some((Value::F64(value), Value::I64(2)))) if value == 1.5
        ));
    }

    #[test]
    fn ordinal_range_splat_honors_value_state_across_method_fallback_11387() {
        let code = vec![
            Instr::PushI64(3),
            Instr::PushI64(3),
            Instr::NewTuple(2),
            Instr::ReturnAny,
        ];
        let mut vm = Vm::new(code, StableRng::new(0));
        let mut first = test_function("iterate", vec![JuliaType::Any]);
        first.code_end = 4;
        vm.functions.push(Rc::new(first));
        vm.function_name_index
            .insert("iterate".to_string(), vec![0]);

        let values = ready_values(vm.prepare_splat_arguments(
            vec![Value::Range(RangeValue::unit_range(3.0, 5.0))],
            &[true],
        ));
        assert!(matches!(
            values.as_slice(),
            [Value::I64(3), Value::I64(4), Value::I64(5)]
        ));
    }

    #[test]
    fn iterate_result_scalar_and_one_field_raise_bounds_11372() {
        let vm = Vm::new(Vec::new(), StableRng::new(0));
        assert!(matches!(
            vm.julia_nth_field_checked(&Value::I64(1), 0),
            Err(VmError::TupleIndexOutOfBounds {
                index: 1,
                length: 0
            })
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&Value::Tuple(TupleValue::new(vec![Value::I64(1)])), 1),
            Err(VmError::TupleIndexOutOfBounds {
                index: 2,
                length: 1
            })
        ));
    }

    #[test]
    fn iterate_result_accepts_extra_tuple_and_namedtuple_fields_11372() {
        let vm = Vm::new(Vec::new(), StableRng::new(0));
        let tuple = Value::Tuple(TupleValue::new(vec![
            Value::I64(11),
            Value::I64(2),
            Value::I64(99),
        ]));
        let named = Value::NamedTuple(NamedTupleValue {
            names: vec![
                "value".to_string(),
                "state".to_string(),
                "extra".to_string(),
            ],
            values: vec![Value::I64(12), Value::I64(3), Value::I64(100)],
        });
        assert!(matches!(
            vm.julia_nth_field_checked(&tuple, 0),
            Ok(Value::I64(11))
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&tuple, 1),
            Ok(Value::I64(2))
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&named, 0),
            Ok(Value::I64(12))
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&named, 1),
            Ok(Value::I64(3))
        ));
    }

    #[test]
    fn iterate_result_accepts_struct_and_struct_ref_fields_11372() {
        let direct = StructInstance::with_name(
            0,
            "Step11372".to_string(),
            vec![Value::I64(13), Value::I64(4), Value::I64(101)],
        );
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.struct_heap.push(StructInstance::with_name(
            0,
            "MutableStep11372".to_string(),
            vec![Value::I64(14), Value::I64(5), Value::I64(102)],
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&Value::Struct(direct.clone()), 0),
            Ok(Value::I64(13))
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&Value::Struct(direct), 1),
            Ok(Value::I64(4))
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&Value::StructRef(0), 0),
            Ok(Value::I64(14))
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&Value::StructRef(0), 1),
            Ok(Value::I64(5))
        ));
        assert!(matches!(
            vm.julia_nth_field_checked(&Value::StructRef(99), 0),
            Err(VmError::InternalError(_))
        ));
    }

    #[test]
    fn generic_positional_splat_reuses_step_root_11372() -> Result<(), VmError> {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let base = vm.begin_transient_root_frame();
        let source = vm.push_transient_root(Value::Range(RangeValue::unit_range(1.0, 10_000.0)))?;
        let roots = match vm.prepare_splat_argument_roots(&[source], &[true]) {
            Ok(SplatPreparation::Ready(roots)) => roots,
            Ok(SplatPreparation::Raised) => panic!("long positional splat unexpectedly raised"),
            Err(err) => panic!("long positional splat failed: {err}"),
        };

        assert_eq!(roots.len(), 10_000);
        assert_eq!(vm.transient_roots.len(), base + 10_002);
        vm.end_transient_root_frame(base);
        assert_eq!(vm.transient_roots.len(), base);
        Ok(())
    }

    #[test]
    fn duplicate_keyword_stream_reuses_scratch_and_value_roots_11372() -> Result<(), VmError> {
        let entries = (0..1_000)
            .map(|value| {
                Value::Tuple(TupleValue::new(vec![
                    Value::Symbol(SymbolValue::new("same")),
                    Value::I64(value),
                ]))
            })
            .collect();
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        let base = vm.begin_transient_root_frame();
        let source = vm.push_transient_root(Value::Tuple(TupleValue::new(entries)))?;
        let kwargs =
            match vm.prepare_kwarg_value_roots(&["options".to_string()], &[true], &[source]) {
                Ok(SplatPreparation::Ready(kwargs)) => kwargs,
                Ok(SplatPreparation::Raised) => panic!("long keyword splat unexpectedly raised"),
                Err(err) => panic!("long keyword splat failed: {err}"),
            };

        assert_eq!(kwargs.len(), 1);
        let same_root = *kwargs.get("same").expect("kwargs must contain \"same\"");
        let value = vm.clone_transient_root(same_root)?;
        assert!(matches!(value, Value::I64(999)));
        assert_eq!(vm.transient_roots.len(), base + 6);
        vm.end_transient_root_frame(base);
        assert_eq!(vm.transient_roots.len(), base);
        Ok(())
    }
}
