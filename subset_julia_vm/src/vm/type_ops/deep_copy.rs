//! Deep copy operations for values.

use crate::rng::RngLike;
use crate::vm::error::VmError;
use crate::vm::value::{
    new_array_ref, ClosureValue, ComposedFunctionValue, DictValue, ExprValue,
    NamedTupleValue, PairsValue, SetValue, StructInstance, TupleValue, Value,
};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    /// Recursively deep copy a value.
    pub(in crate::vm) fn deep_copy_value(&mut self, val: &Value) -> Result<Value, VmError> {
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
            Value::Str(s) => Value::Str(s.clone()),
            Value::Char(c) => Value::Char(*c),
            Value::Nothing => Value::Nothing,
            Value::Missing => Value::Missing,
            Value::Undef => Value::Undef,
            Value::SliceAll => Value::SliceAll,

            // Array - deep copy elements
            Value::Array(arr) => {
                let borrowed = arr.borrow();
                Value::Array(new_array_ref(crate::vm::value::ArrayValue::new(
                    borrowed.data.clone(),
                    borrowed.shape.clone(),
                )))
            }

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

            // Dict - deep copy entries
            Value::Dict(d) => {
                let mut new_dict =
                    DictValue::with_type_params_opt(d.key_type.clone(), d.value_type.clone());
                for (k, v) in d.iter() {
                    let new_v = self.deep_copy_value(v)?;
                    new_dict.insert(k.clone(), new_v);
                }
                Value::Dict(Box::new(new_dict))
            }

            // Set - just clone elements (DictKey is cloneable)
            Value::Set(s) => Value::Set(SetValue {
                elements: s.elements.clone(),
            }),

            // Range - just clone (immutable)
            Value::Range(r) => Value::Range(r.clone()),

            // Ref - deep copy inner
            Value::Ref(inner) => {
                let new_inner = self.deep_copy_value(inner)?;
                Value::Ref(Box::new(new_inner))
            }

            // Complex types that are typically not deep copied
            Value::Rng(rng) => Value::Rng(rng.clone()),
            Value::Generator(g) => Value::Generator(g.clone()),
            Value::DataType(dt) => Value::DataType(dt.clone()),
            Value::Module(m) => Value::Module(m.clone()),
            Value::Function(f) => Value::Function(f.clone()),
            Value::Closure(c) => {
                // Deep copy captured values
                let new_captures: Result<Vec<(String, Value)>, VmError> = c
                    .captures
                    .iter()
                    .map(|(name, v)| Ok((name.clone(), self.deep_copy_value(v)?)))
                    .collect();
                Value::Closure(ClosureValue::new(c.name.clone(), new_captures?))
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
                let new_args: Result<Vec<Value>, VmError> =
                    e.args.iter().map(|a| self.deep_copy_value(a)).collect();
                Value::Expr(ExprValue {
                    head: e.head.clone(),
                    args: new_args?,
                })
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
                Value::Memory(crate::vm::value::new_memory_ref(mem_borrow.copy()))
            }
        })
    }

}
