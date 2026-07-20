//! Deep copy operations for values.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;
use crate::vm::error::VmError;
use crate::vm::value::{
    ArrayData, ArrayElementType, ArrayValue, ClosureValue, ComposedFunctionValue, ExprValue,
    MemoryValue, NamedTupleValue, PairsValue, StructInstance, TupleValue, Value,
};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    /// Recursively deep copy a value.
    pub(in crate::vm) fn deep_copy_value(&mut self, val: &Value) -> Result<Value, VmError> {
        // A native-array *input* (transient host/cache carrier) deep-copies to
        // the MemoryRef-backed `Array{T,N}` wrapper so this method no longer
        // *produces* the carrier (Issue #6806). Wrapper inputs are deep-copied by
        // the `Struct`/`StructRef` arms of the match below. `arr` borrows `val`
        // (a parameter, not `self`), so the owned `copied` can be wrapped through
        // the `&mut self` helper without a borrow conflict.
        if let Some(arr) = crate::vm::value::native_array_value_ref(val) {
            let copied = self.deep_copy_native_array(&arr.borrow())?;
            return self.array_value_to_wrapper(copied);
        }
        Ok(match val {
            // Primitive types - just clone
            Value::I8(v) => Value::I8(*v),
            Value::I16(v) => Value::I16(*v),
            Value::I32(v) => Value::I32(*v),
            Value::I64(v) => Value::I64(*v),
            Value::I128(v) => Value::I128(*v),
            Value::U8(v) => Value::U8(*v),
            Value::U16(v) => Value::U16(*v),
            Value::U32(v) => Value::U32(*v),
            Value::U64(v) => Value::U64(*v),
            Value::U128(v) => Value::U128(*v),
            Value::Bool(v) => Value::Bool(*v),
            Value::F16(v) => Value::F16(*v),
            Value::F32(v) => Value::F32(*v),
            Value::F64(v) => Value::F64(*v),
            Value::BigInt(v) => Value::BigInt(v.clone()),
            Value::BigFloat(v) => Value::BigFloat(v.clone()),
            Value::Str(s) => Value::str_new(s.clone()),
            Value::Char(c) => Value::Char(*c),
            Value::Nothing => Value::Nothing,
            Value::Missing => Value::Missing,
            Value::Undef => Value::Undef,
            Value::SliceAll => Value::SliceAll,

            // Tuple - deep copy elements
            Value::Tuple(t) => {
                let elements: Result<Vec<Value>, VmError> =
                    t.elements.iter().map(|e| self.deep_copy_value(e)).collect();
                Value::Tuple(TupleValue {
                    elements: elements?,
                })
            }

            // NamedTuple - deep copy values
            Value::NamedTuple(nt) => {
                let values: Result<Vec<Value>, VmError> =
                    nt.values.iter().map(|v| self.deep_copy_value(v)).collect();
                Value::NamedTuple(NamedTupleValue {
                    names: nt.names.clone(),
                    values: values?,
                })
            }

            // Struct - create a new copy on the heap
            Value::Struct(si) => {
                let values: Result<Vec<Value>, VmError> =
                    si.values.iter().map(|f| self.deep_copy_value(f)).collect();
                Value::Struct(StructInstance {
                    type_id: si.type_id,
                    struct_name: si.struct_name.clone(),
                    values: values?,
                })
            }

            // StructRef - create a new instance on the heap
            Value::StructRef(idx) => {
                // Clone values first to release the borrow on struct_heap
                let (type_id, struct_name, old_values) =
                    if let Some(si) = self.struct_heap.get(*idx) {
                        (si.type_id, si.struct_name.clone(), si.values.clone())
                    } else {
                        return Ok(Value::StructRef(*idx)); // Keep as-is if not found
                    };

                // Now we can safely call deep_copy_value
                let mut new_values = Vec::new();
                for v in &old_values {
                    new_values.push(self.deep_copy_value(v)?);
                }

                let new_si = StructInstance {
                    type_id,
                    struct_name,
                    values: new_values,
                };
                let new_idx = self.struct_heap.len();
                self.struct_heap.push(new_si);
                Value::StructRef(new_idx)
            }

            // Dict - deep copy entries into an INDEPENDENT shared dict (#5675).

            // Set - just clone elements and element type carrier (DictKey is cloneable)

            // Range - just clone (immutable)
            Value::Range(r) => Value::Range(r.clone()),

            // Ref - deep copy inner into a fresh cell (Issue #5130)
            Value::Ref(inner) => {
                let new_inner = self.deep_copy_value(&inner.borrow())?;
                crate::vm::value::new_ref(new_inner)
            }

            // Complex types that are typically not deep copied
            Value::Rng(rng) => Value::Rng(rng.clone()),
            Value::Generator(g) => Value::Generator(g.clone()),
            Value::DataType(dt) => Value::DataType(dt.clone()),
            Value::RuntimeTypeVar(tv) => Value::RuntimeTypeVar(tv.clone()),
            Value::RuntimeTypeName(tn) => Value::RuntimeTypeName(tn.clone()),
            Value::Module(m) => Value::Module(m.clone()),
            Value::Function(f) => Value::Function(f.clone()),
            Value::Closure(c) => {
                // Deep copy captured values
                let new_captures: Result<Vec<(String, Value)>, VmError> = c
                    .captures
                    .iter()
                    .map(|(name, v)| Ok((name.clone(), self.deep_copy_value(v)?)))
                    .collect();
                let new_captures = new_captures?;
                Value::Closure(if let Some(indices) = &c.candidate_indices {
                    ClosureValue::with_candidates_and_identity(
                        c.name.clone(),
                        new_captures,
                        indices.clone(),
                        c.singleton_identity().clone(),
                    )
                } else {
                    ClosureValue::new(c.name.clone(), new_captures)
                })
            }
            Value::ComposedFunction(cf) => {
                // Deep copy both outer and inner functions
                let outer = self.deep_copy_value(&cf.outer)?;
                let inner = self.deep_copy_value(&cf.inner)?;
                Value::ComposedFunction(ComposedFunctionValue::new(outer, inner))
            }
            Value::IO(io) => Value::IO(io.clone()),

            // Macro system types - deep copy
            Value::Symbol(s) => Value::Symbol(s.clone()),
            Value::Expr(e) => {
                let new_args: Result<Vec<Value>, VmError> = e
                    .args_snapshot()
                    .iter()
                    .map(|a| self.deep_copy_value(a))
                    .collect();
                Value::Expr(ExprValue::new(e.head.clone(), new_args?))
            }
            Value::QuoteNode(inner) => {
                let new_inner = self.deep_copy_value(inner)?;
                Value::QuoteNode(Box::new(new_inner))
            }
            Value::LineNumberNode(ln) => Value::LineNumberNode(ln.clone()),
            Value::GlobalRef(gr) => Value::GlobalRef(gr.clone()),

            // Base.Pairs type - deep copy values
            Value::Pairs(p) => {
                let values: Result<Vec<Value>, VmError> = p
                    .data
                    .values
                    .iter()
                    .map(|v| self.deep_copy_value(v))
                    .collect();
                Value::Pairs(PairsValue {
                    data: NamedTupleValue {
                        names: p.data.names.clone(),
                        values: values?,
                    },
                })
            }
            // Regex types - just clone (patterns are immutable)
            Value::Regex(r) => Value::Regex(r.clone()),
            Value::RegexMatch(m) => Value::RegexMatch(m.clone()),
            // Enum type - just clone (value is immutable)
            Value::Enum { type_name, value } => Value::Enum {
                type_name: type_name.clone(),
                value: *value,
            },
            // Memory type - deep copy the buffer
            Value::Memory(mem) => {
                let mem_borrow = mem.borrow();
                Value::Memory(crate::vm::value::new_memory_ref(
                    self.deep_copy_memory_value(&mem_borrow)?,
                ))
            }
            Value::MemoryRef(memref) => {
                let mem_borrow = memref.memory.borrow();
                Value::MemoryRef(Box::new(crate::vm::value::MemoryRefValue {
                    memory: crate::vm::value::new_memory_ref(
                        self.deep_copy_memory_value(&mem_borrow)?,
                    ),
                    offset: memref.offset,
                }))
            }
            // The legacy native-array carrier is filtered out by the
            // early-return above (Issue #3908). This wildcard satisfies
            // Rust's exhaustiveness checking and provides a safe default
            // for any future `Value` variant: clone the value as-is.
            other => other.clone(),
        })
    }

    fn deep_copy_native_array(&mut self, source: &ArrayValue) -> Result<ArrayValue, VmError> {
        let element_type = source.element_type();
        if element_type != ArrayElementType::Any {
            return ArrayValue::memory_first_copy_from_array(source);
        }

        let mut copy =
            ArrayValue::memory_first_with_capacity(ArrayElementType::Any, source.element_count());
        for value in source.to_logical_value_vec()? {
            let copied_value = self.deep_copy_value(&value)?;
            copy.push(copied_value)?;
        }
        copy.shape = source.shape.clone();
        Ok(copy)
    }

    fn deep_copy_memory_value(&mut self, source: &MemoryValue) -> Result<MemoryValue, VmError> {
        let ArrayData::Any(values) = &source.data else {
            return Ok(source.copy());
        };

        let copied_values: Result<Vec<Value>, VmError> = values
            .iter()
            .map(|value| self.deep_copy_value(value))
            .collect();
        Ok(MemoryValue::new(
            ArrayData::Any(copied_values?),
            source.element_type.clone(),
            source.length,
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use crate::rng::StableRng;
    use crate::vm::value::{
        array_wrapper_value_to_array_value, native_array_value_from_array, new_ref,
        ArrayElementType, ArrayValue, Value,
    };
    use crate::vm::Vm;

    #[test]
    fn deep_copy_array_uses_logical_memory_first_copy() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        let original = ArrayValue::complex_f64(vec![1.0, 2.0, 3.0, 4.0], vec![2]);
        // A native-array input now deep-copies to the `Array{T,N}` wrapper
        // (Issue #6806); materialize it back to an `ArrayValue` for the asserts.
        let copied = vm
            .deep_copy_value(&native_array_value_from_array(original))
            .unwrap();

        let arr = array_wrapper_value_to_array_value(&copied, &vm.struct_heap)
            .unwrap()
            .expect("deep copy of a native array should produce an Array wrapper");
        assert_eq!(arr.shape, vec![2]);
        assert_eq!(arr.element_type(), ArrayElementType::ComplexF64);
        assert_eq!(
            arr.element_type_override,
            Some(ArrayElementType::ComplexF64)
        );
        assert_eq!(arr.to_logical_value_vec().unwrap().len(), 2);
    }

    #[test]
    fn deep_copy_any_array_recursively_copies_ref_elements() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        let original_ref_value = new_ref(Value::I64(1));
        let original_ref = if let Value::Ref(cell) = &original_ref_value {
            cell.clone()
        } else {
            assert!(matches!(original_ref_value, Value::Ref(_)));
            return;
        };
        let original = ArrayValue::any_vector(vec![original_ref_value.clone()]);

        let copied = vm
            .deep_copy_value(&native_array_value_from_array(original))
            .unwrap();
        let arr = array_wrapper_value_to_array_value(&copied, &vm.struct_heap)
            .unwrap()
            .expect("deep copy of Any array should produce an Array wrapper");
        let elements = arr.to_logical_value_vec().unwrap();
        let copied_ref = if let Value::Ref(cell) = &elements[0] {
            cell.clone()
        } else {
            assert!(matches!(elements[0], Value::Ref(_)));
            return;
        };

        *copied_ref.borrow_mut() = Value::I64(99);
        assert!(matches!(*original_ref.borrow(), Value::I64(1)));
        assert!(matches!(*copied_ref.borrow(), Value::I64(99)));
    }

    #[test]
    fn deep_copy_array_wrapper_recursively_copies_any_memory_refs() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        let original_ref_value = new_ref(Value::I64(1));
        let original_ref = if let Value::Ref(cell) = &original_ref_value {
            cell.clone()
        } else {
            assert!(matches!(original_ref_value, Value::Ref(_)));
            return;
        };
        let original = ArrayValue::any_vector(vec![original_ref_value.clone()]);
        let wrapper = vm.array_value_to_wrapper(original).unwrap();

        let copied = vm.deep_copy_value(&wrapper).unwrap();
        let arr = array_wrapper_value_to_array_value(&copied, &vm.struct_heap)
            .unwrap()
            .expect("deep copy of Array wrapper should remain an Array wrapper");
        let elements = arr.to_logical_value_vec().unwrap();
        let copied_ref = if let Value::Ref(cell) = &elements[0] {
            cell.clone()
        } else {
            assert!(matches!(elements[0], Value::Ref(_)));
            return;
        };

        *copied_ref.borrow_mut() = Value::I64(99);
        assert!(matches!(*original_ref.borrow(), Value::I64(1)));
        assert!(matches!(*copied_ref.borrow(), Value::I64(99)));
    }
}
