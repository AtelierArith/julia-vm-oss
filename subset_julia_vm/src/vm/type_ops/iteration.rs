//! Iteration operations for values.

// SAFETY: i64→usize casts for range/array/struct iteration are from `r.length()` (≥ 0)
// or from iteration state indices that are non-negative by construction.
#![allow(clippy::cast_sign_loss)]

use crate::rng::RngLike;
use crate::vm::error::VmError;
use crate::vm::exec::array_basic::array_element_type_from_julia_type;
use crate::vm::field_indices::{
    ARRAY_FIRST_DIM_INDEX, ARRAY_SECOND_DIM_INDEX, FIRST_FIELD_INDEX, FOURTH_FIELD_INDEX,
    SECOND_FIELD_INDEX, THIRD_FIELD_INDEX,
};
use crate::vm::hof_exec::state::HofOpKind;
use crate::vm::value::is_native_array_value;
use crate::vm::value::{
    array_wrapper_value_from_array_value, array_wrapper_value_to_array_value,
    native_array_value_from_array, native_array_value_ref, ArrayElementType, ArrayValue,
    FunctionValue, GeneratorCallable, GeneratorValue, PairsValue, StructInstance, SymbolValue,
    TupleValue, Value,
};
use crate::vm::Vm;

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

    fn is_array_wrapper_struct_name(name: &str) -> bool {
        name == "Array" || name.starts_with("Array{")
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
            Value::U8(_) => ArrayElementType::U8,
            Value::U16(_) => ArrayElementType::U16,
            Value::U32(_) => ArrayElementType::U32,
            Value::U64(_) => ArrayElementType::U64,
            Value::U128(_) => ArrayElementType::U128,
            Value::F32(_) => ArrayElementType::F32,
            Value::F64(_) => ArrayElementType::F64,
            Value::Bool(_) => ArrayElementType::Bool,
            Value::Str(_) => ArrayElementType::String,
            Value::Char(_) => ArrayElementType::Char,
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
            ArrayElementType::F32 | ArrayElementType::F64 => {
                Some(&["AbstractFloat", "Real", "Number", "Any"])
            }
            ArrayElementType::Abstract(name) => match name.as_str() {
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
                    let elem =
                        mem_borrow
                            .data
                            .get_value(0)
                            .ok_or_else(|| VmError::IndexOutOfBounds {
                                indices: vec![1],
                                shape: vec![len],
                            })?;
                    Ok(Some(Some((elem, Value::I64(1)))))
                }
            }
            Value::Range(r) => {
                if r.length() as usize == 0 {
                    Ok(Some(None))
                } else {
                    // Issue #3550: preserve the declared range element type.
                    let elem = r.typed_element(r.start);
                    Ok(Some(Some((elem, Value::I64(1)))))
                }
            }
            Value::Tuple(t) => {
                if t.elements.is_empty() {
                    Ok(Some(None))
                } else {
                    Ok(Some(Some((t.elements[0].clone(), Value::I64(2)))))
                }
            }
            Value::Str(s) => match s.chars().next() {
                None => Ok(Some(None)),
                Some(first_char) => Ok(Some(Some((Value::Char(first_char), Value::I64(1))))),
            },
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
        // Only handle the fast-path collections with an I64 state. Anything else
        // (including non-I64 states) defers to the generic tuple path.
        let idx = match (coll, state) {
            (Value::Range(_), Value::I64(i))
            | (Value::Tuple(_), Value::I64(i))
            | (Value::Str(_), Value::I64(i))
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
                if idx >= mem_borrow.len() {
                    Ok(Some(None))
                } else {
                    let elem = mem_borrow.data.get_value(idx).ok_or_else(|| {
                        VmError::IndexOutOfBounds {
                            indices: vec![idx as i64 + 1],
                            shape: vec![mem_borrow.len()],
                        }
                    })?;
                    Ok(Some(Some((elem, Value::I64((idx + 1) as i64)))))
                }
            }
            Value::Range(r) => {
                if idx >= r.length() as usize {
                    Ok(Some(None))
                } else {
                    let val = r.start + (idx as f64) * r.step;
                    // Issue #3550: preserve declared range element type.
                    let elem = r.typed_element(val);
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
            Value::Str(s) => {
                let chars: Vec<char> = s.chars().collect();
                if idx >= chars.len() {
                    Ok(Some(None))
                } else {
                    Ok(Some(Some((
                        Value::Char(chars[idx]),
                        Value::I64((idx + 1) as i64),
                    ))))
                }
            }
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
            name if Self::is_array_wrapper_struct_name(name) => {
                return self.iterate_array_wrapper_fields(&s.values, 0);
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
                    let elem =
                        mem_borrow
                            .data
                            .get_value(0)
                            .ok_or_else(|| VmError::IndexOutOfBounds {
                                indices: vec![1],
                                shape: vec![len],
                            })?;
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, Value::I64(1)],
                    }))
                }
            }
            Value::Range(r) => {
                let len = r.length() as usize;
                if len == 0 {
                    Ok(Value::Nothing)
                } else {
                    // Issue #3550: preserve the declared range element type so
                    // `for x in UInt8(1):UInt8(3)` yields `UInt8` values, not `Int64`.
                    let elem = r.typed_element(r.start);
                    let state = Value::I64(1);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, state],
                    }))
                }
            }
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
            Value::Str(s) => {
                if s.is_empty() {
                    Ok(Value::Nothing)
                } else {
                    // Return first character as Char (Julia's behavior)
                    // Safety: guarded by !s.is_empty() above
                    let first_char = match s.chars().next() {
                        Some(c) => c,
                        None => return Ok(Value::Nothing),
                    };
                    let elem = Value::Char(first_char);
                    let state = Value::I64(1);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, state],
                    }))
                }
            }
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
        match &*s.struct_name {
            name if Self::is_array_wrapper_struct_name(name) => {
                if idx == 0 {
                    Ok(Value::Nothing)
                } else {
                    self.iterate_array_wrapper_fields(&s.values, idx - 1)
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
                if idx >= mem_borrow.len() {
                    Ok(Value::Nothing)
                } else {
                    let elem = mem_borrow.data.get_value(idx).ok_or_else(|| {
                        VmError::IndexOutOfBounds {
                            indices: vec![idx as i64 + 1],
                            shape: vec![mem_borrow.len()],
                        }
                    })?;
                    let next_state = Value::I64((idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, next_state],
                    }))
                }
            }
            Value::Range(r) => {
                let len = r.length() as usize;
                if idx >= len {
                    Ok(Value::Nothing)
                } else {
                    let val = r.start + (idx as f64) * r.step;
                    // Issue #3550: preserve declared range element type during iteration.
                    let elem = r.typed_element(val);
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
            Value::Str(s) => {
                let chars: Vec<char> = s.chars().collect();
                if idx >= chars.len() {
                    Ok(Value::Nothing)
                } else {
                    // Return character as Char (Julia's behavior)
                    let elem = Value::Char(chars[idx]);
                    let next_state = Value::I64((idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, next_state],
                    }))
                }
            }
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
                // CollectFallback: struct-native-iterator-materialization
                self.collect_struct_iterator(&s.struct_name, &s.values)
            }
            Value::StructRef(idx) => {
                let Some((struct_name, values)) = self
                    .struct_heap
                    .get(*idx)
                    .map(|s| (s.struct_name.clone(), s.values.clone()))
                else {
                    return Err(VmError::TypeError(format!(
                        "collect: invalid StructRef({})",
                        idx
                    )));
                };
                self.collect_struct_iterator(&struct_name, &values)
            }
            // Generator passed as a plain iterable to the synchronous collect
            // boundary (Issue #5138). This happens when an *eager* generator
            // expression `(expr for x in it)` (whose body is not a plain unary
            // call) is consumed by `map`, a comprehension, or another generator
            // lowered to `collect(Generator(f, gen))`. An eager generator already
            // holds its materialized values in `g.iter`, so collecting it just
            // re-materializes that array. Function-backed generators need the
            // asynchronous `collect_generator` HOF path (frame re-entry) and are
            // intentionally not handled here.
            Value::Generator(g) if matches!(g.callable, GeneratorCallable::Eager) => {
                // CollectFallback: eager-generator-iterable-materialization
                self.collect_iterator(g.iter.as_ref())
            }
            _ => Err(VmError::TypeError(format!(
                "collect: unsupported iterator type {:?}",
                iter
            ))),
        }
    }

    fn collect_struct_iterator(
        &mut self,
        struct_name: &str,
        fields: &[Value],
    ) -> Result<Value, VmError> {
        if Self::is_array_wrapper_struct_name(struct_name) {
            return self.collect_array_wrapper_fields(fields);
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
            return self.collect_zip_fields(fields);
        }
        if struct_name == "Enumerate" || struct_name.starts_with("Enumerate{") {
            // CollectFallback: enumerate-struct-legacy-materialization
            return self.collect_enumerate_fields(fields);
        }
        if struct_name == "Rest" || struct_name.starts_with("Rest{") {
            // CollectFallback: rest-struct-legacy-materialization
            return self.collect_rest_fields(fields);
        }
        if struct_name == "LogRange" || struct_name.starts_with("LogRange{") {
            // CollectFallback: logrange-struct-materialization
            return self.collect_logrange_fields(fields);
        }
        Err(VmError::TypeError(format!(
            "collect: unsupported iterator type {}",
            struct_name
        )))
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
            _ => Err(VmError::TypeError(format!(
                "type Base.Generator has no field {field_name}"
            ))),
        }
    }

    pub(in crate::vm) fn generator_projected_field_by_index(
        &self,
        generator: &GeneratorValue,
        field_idx: usize,
    ) -> Result<Value, VmError> {
        match field_idx {
            0 => self.generator_callable_field_value(&generator.callable),
            1 => Ok(generator.iter.as_ref().clone()),
            _ => Err(VmError::FieldIndexOutOfBounds {
                index: field_idx,
                field_count: 2,
            }),
        }
    }

    fn generator_callable_field_value(
        &self,
        callable: &GeneratorCallable,
    ) -> Result<Value, VmError> {
        match callable {
            GeneratorCallable::FunctionIndex(func_index) => self
                .functions
                .get(*func_index)
                .map(|function| Value::Function(FunctionValue::new(function.name.clone())))
                .ok_or_else(|| {
                    VmError::TypeError(format!(
                        "Base.Generator callable references invalid function index {func_index}"
                    ))
                }),
            GeneratorCallable::TypeObject(julia_type) => {
                Ok(Value::DataType(Box::new(julia_type.clone())))
            }
            GeneratorCallable::RuntimeValue(value) => Ok(value.as_ref().clone()),
            GeneratorCallable::TupleSplatFunctionIndex(_)
            | GeneratorCallable::TupleSplatTypeObject(_)
            | GeneratorCallable::TupleSplatRuntimeValue(_)
            | GeneratorCallable::FilteredFunctionIndex { .. }
            | GeneratorCallable::Eager => Err(VmError::TypeError(
                "Base.Generator callable field is not representable as a direct callable value"
                    .to_string(),
            )),
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
                    result_element_type.unwrap_or_else(|| ArrayElementType::UnionOf(Vec::new()));
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
                    let element_type = result_element_type
                        .unwrap_or_else(|| ArrayElementType::UnionOf(Vec::new()));
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
                    let element_type = result_element_type.unwrap_or(ArrayElementType::Any);
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
