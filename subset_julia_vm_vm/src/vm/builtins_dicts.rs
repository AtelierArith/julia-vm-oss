//! Dict builtin functions for the VM.
//!
//! With `Value::Dict` retired (Issue #6731), every concrete `Dict` is a
//! pure-Julia `Dict{K,V}` StructRef. These `BuiltinId::Dict*` handlers are thin
//! struct-dispatch trampolines: they forward to the pure-Julia `Dict{K,V}`
//! methods (`get`/`getindex`/`setindex!`/`delete!`/`haskey`/`get!`/`merge!`/
//! `empty!`/`pop!`) and to user-defined methods on non-Dict structs. `keys`/
//! `values`/`pairs` additionally serve NamedTuple/Array/Tuple/Memory arguments.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;
use crate::vm::value::is_native_array_value;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::util::is_dict_type_name;
use super::value::{
    array_wrapper_value_to_array_value, native_array_value_ref, RangeValue, TupleValue, Value,
};
use super::Vm;

impl<R: RngLike> Vm<R> {
    /// Check if the dict argument (first on stack) is a StructRef Dict
    /// and dispatch to the corresponding Pure Julia function if so.
    /// Returns `Ok(true)` if dispatched, `Ok(false)` if not StructRef Dict.
    /// (Issue #2748)
    fn try_dispatch_struct_dict(
        &mut self,
        func_names: &[&str],
        argc: usize,
    ) -> Result<bool, VmError> {
        let stack_len = self.stack.len();
        if stack_len < argc {
            return Ok(false);
        }
        // The first argument (dict) is at stack_len - argc
        let dict_pos = stack_len - argc;
        let is_struct_dict = match &self.stack[dict_pos] {
            Value::StructRef(idx) => self
                .struct_heap
                .get(*idx)
                .map(|s| is_dict_type_name(&s.struct_name))
                .unwrap_or(false),
            _ => false,
        };
        if !is_struct_dict {
            return Ok(false);
        }
        // Pop all args (in reverse stack order) and reverse to calling convention order
        let mut args = Vec::with_capacity(argc);
        for _ in 0..argc {
            args.push(self.stack.pop_value()?);
        }
        args.reverse();
        if let Some(func_index) = self.find_best_method_index(func_names, &args) {
            self.start_function_call(func_index, args)?;
            return Ok(true);
        }
        let type_name = self.get_type_name(&args[0]);
        Err(VmError::MethodError(format!(
            "no method matching {}({})",
            func_names[0], type_name
        )))
    }

    /// Check if the first argument is a non-Dict StructRef and dispatch to user-defined methods.
    /// Returns `Ok(true)` if dispatched, `Ok(false)` if first arg is not a non-Dict StructRef.
    /// If a non-Dict StructRef is found but no method matches, returns a MethodError.
    /// (Issue #3152)
    fn try_dispatch_struct_non_dict(
        &mut self,
        func_names: &[&str],
        argc: usize,
    ) -> Result<bool, VmError> {
        let stack_len = self.stack.len();
        if stack_len < argc {
            return Ok(false);
        }
        // The first argument is at stack_len - argc
        let first_arg_pos = stack_len - argc;
        if array_wrapper_value_to_array_value(&self.stack[first_arg_pos], &self.struct_heap)?
            .is_some()
        {
            return Ok(false);
        }
        let is_non_dict_struct = match &self.stack[first_arg_pos] {
            Value::StructRef(idx) => self
                .struct_heap
                .get(*idx)
                .map(|s| !is_dict_type_name(&s.struct_name))
                .unwrap_or(false),
            _ => false,
        };
        if !is_non_dict_struct {
            return Ok(false);
        }
        // Pop all args (in reverse stack order) and reverse to calling convention order
        let mut args = Vec::with_capacity(argc);
        for _ in 0..argc {
            args.push(self.stack.pop_value()?);
        }
        args.reverse();
        if let Some(func_index) = self.find_best_method_index(func_names, &args) {
            self.start_function_call(func_index, args)?;
            return Ok(true);
        }
        let type_name = self.get_type_name(&args[0]);
        Err(VmError::MethodError(format!(
            "no method matching {}({})",
            func_names[0], type_name
        )))
    }

    /// Dispatch error for a Dict builtin reached with a non-Dict, non-struct
    /// first argument. After `Value::Dict` removal (Issue #6731) the carrier
    /// fast paths are gone; the struct-dispatch trampolines above handle every
    /// real `Dict{K,V}`, so reaching here means no method matched.
    fn dict_no_method(&mut self, func: &str, argc: usize) -> VmError {
        let stack_len = self.stack.len();
        let type_name = if stack_len >= argc && argc > 0 {
            crate::vm::util::value_type_name(&self.stack[stack_len - argc]).to_string()
        } else {
            "?".to_string()
        };
        VmError::MethodError(format!("no method matching {}({})", func, type_name))
    }

    /// Execute Dict builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a Dict builtin.
    pub(super) fn execute_builtin_dicts(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::DictGet => {
                // get(dict, key[, default]) — pure Dict{K,V} dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["get", "Base.get"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch: e.g. get(::IOContext, key, default) (Issue #3152)
                if self.try_dispatch_struct_non_dict(&["get", "Base.get"], argc)? {
                    return Ok(Some(()));
                }
                return Err(self.dict_no_method("get", argc));
            }

            BuiltinId::DictGetkey => {
                if self.try_dispatch_struct_dict(&["getkey", "Base.getkey"], argc)? {
                    return Ok(Some(()));
                }
                if self.try_dispatch_struct_non_dict(&["getkey", "Base.getkey"], argc)? {
                    return Ok(Some(()));
                }
                return Err(self.dict_no_method("getkey", argc));
            }

            BuiltinId::DictSet => {
                // setindex!(dict, value, key) / dict[key] = value (Issue #2748, #3169)
                // Compilation order: push collection (args[0]), key (args[2]), value (args[1]).
                // Stack: [collection (bottom), key (middle), value (top)].
                // After pop+reverse: [collection, key, value]; swap to the Julia
                // convention setindex!(collection, value, key).
                let stack_len = self.stack.len();
                if stack_len >= argc
                    && argc > 0
                    && matches!(&self.stack[stack_len - argc], Value::StructRef(_))
                {
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(self.stack.pop_value()?);
                    }
                    args.reverse();
                    if args.len() >= 3 {
                        args.swap(1, 2);
                    }
                    let func_names = &["setindex!", "Base.setindex!"];
                    if let Some(func_index) = self.find_best_method_index(func_names, &args) {
                        self.start_function_call(func_index, args)?;
                        return Ok(Some(()));
                    }
                    let type_name = self.get_type_name(&args[0]);
                    return Err(VmError::MethodError(format!(
                        "no method matching {}({})",
                        func_names[0], type_name
                    )));
                }
                return Err(self.dict_no_method("setindex!", argc));
            }

            BuiltinId::DictDelete => {
                if self.try_dispatch_struct_dict(&["delete!", "Base.delete!"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch: e.g. delete!(::CustomContainer, key) (Issue #3169)
                if self.try_dispatch_struct_non_dict(&["delete!", "Base.delete!"], argc)? {
                    return Ok(Some(()));
                }
                // `Value::Set` was retired (Issue #6732); delete! on a pure Set{T}
                // struct dispatches to the `delete!(::Set, x)` method above.
                return Err(self.dict_no_method("delete!", argc));
            }

            BuiltinId::DictHasKey => {
                if self.try_dispatch_struct_dict(&["haskey", "Base.haskey"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch: e.g. haskey(::IOContext, key) (Issue #3152)
                if self.try_dispatch_struct_non_dict(&["haskey", "Base.haskey"], argc)? {
                    return Ok(Some(()));
                }
                // haskey(m::RegexMatch, key) (Issue #10173): integer key is in
                // bounds of the captures; String/Symbol key names a capture group.
                if argc == 2
                    && self.stack.len() >= 2
                    && matches!(self.stack[self.stack.len() - 2], Value::RegexMatch(_))
                {
                    let key = self.stack.pop_value()?;
                    let receiver = self.stack.pop_value()?;
                    let Value::RegexMatch(m) = receiver else {
                        return Err(VmError::InternalError(
                            "haskey: RegexMatch receiver changed".to_string(),
                        ));
                    };
                    // Integer key (any width, upstream `::Integer`) must be in
                    // bounds; a String/Symbol key names a capture group.
                    let present = if let Some(i) = crate::vm::util::regexmatch_integer_index(&key) {
                        usize::try_from(i).is_ok_and(|idx| idx >= 1 && idx <= m.captures.len())
                    } else if let Value::Symbol(name) = &key {
                        m.capture_index_by_name(name.as_str()).is_some()
                    } else {
                        match key.string_lossy() {
                            Some(name) => m.capture_index_by_name(&name).is_some(),
                            None => false,
                        }
                    };
                    self.stack.push(Value::Bool(present));
                    return Ok(Some(()));
                }
                return Err(self.dict_no_method("haskey", argc));
            }

            BuiltinId::DictKeys => {
                if self.try_dispatch_struct_dict(&["keys", "Base.keys"], argc)? {
                    return Ok(Some(()));
                }
                if self.try_dispatch_struct_non_dict(&["keys", "Base.keys"], argc)? {
                    return Ok(Some(()));
                }
                // keys(namedtuple/array/tuple/memory) — non-Dict collections (Issue #1872)
                let val = self.stack.pop_value()?;
                match val {
                    Value::NamedTuple(nt) => {
                        let keys: Vec<Value> = nt
                            .names
                            .iter()
                            .map(|n| Value::Symbol(super::value::SymbolValue::new(n)))
                            .collect();
                        self.stack.push(Value::Tuple(TupleValue { elements: keys }));
                    }
                    // keys(m::RegexMatch): named groups -> String name, unnamed
                    // -> 1-based Int index (Issue #10173). Element type follows
                    // upstream's `map` typejoin: all-named -> Vector{String},
                    // all-unnamed -> Vector{Int64}, mixed -> Vector{Any}.
                    Value::RegexMatch(m) => {
                        let keys: Vec<Value> = (1..=m.captures.len())
                            .map(
                                |i| match m.capture_names.get(i - 1).and_then(|n| n.as_deref()) {
                                    Some(name) => Value::str_new(name.to_string()),
                                    None => Value::I64(i as i64),
                                },
                            )
                            .collect();
                        // Empty fallback is `Vector{Any}` to match upstream's
                        // `map(::eachindex)` over zero capture groups (`Any[]`).
                        let arr =
                            crate::vm::value::ArrayValue::memory_first_collect_typejoin_values(
                                keys,
                                crate::vm::value::ArrayElementType::Any,
                            )?;
                        self.stack
                            .push(crate::vm::value::native_array_value_from_array(arr));
                    }
                    _ if is_native_array_value(&val) => {
                        let Some(arr) = native_array_value_ref(&val) else {
                            return Err(VmError::TypeError("keys: expected Array".into()));
                        };
                        let len = arr.borrow().len();
                        self.stack
                            .push(Value::Range(RangeValue::unit_range(1.0, len as f64)));
                    }
                    Value::Memory(mem) => {
                        let len = mem.borrow().len();
                        self.stack
                            .push(Value::Range(RangeValue::unit_range(1.0, len as f64)));
                    }
                    _ if array_wrapper_value_to_array_value(&val, &self.struct_heap)?.is_some() => {
                        let arr = array_wrapper_value_to_array_value(&val, &self.struct_heap)?
                            .ok_or_else(|| VmError::TypeError("keys: expected Array".into()))?;
                        self.stack
                            .push(Value::Range(RangeValue::unit_range(1.0, arr.len() as f64)));
                    }
                    Value::Tuple(t) => {
                        let len = t.elements.len();
                        self.stack
                            .push(Value::Range(RangeValue::unit_range(1.0, len as f64)));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "keys: expected Dict, NamedTuple, Array, or Tuple, got {:?}",
                            crate::vm::util::value_type_name(&val)
                        )));
                    }
                }
            }

            BuiltinId::DictValues => {
                if self.try_dispatch_struct_dict(&["values", "Base.values"], argc)? {
                    return Ok(Some(()));
                }
                if self.try_dispatch_struct_non_dict(&["values", "Base.values"], argc)? {
                    return Ok(Some(()));
                }
                // values(namedtuple/array/tuple/memory) — non-Dict collections (Issue #1872)
                let val = self.stack.pop_value()?;
                match val {
                    Value::NamedTuple(nt) => {
                        self.stack.push(Value::Tuple(TupleValue {
                            elements: nt.values,
                        }));
                    }
                    _ if is_native_array_value(&val) => {
                        self.stack.push(val);
                    }
                    Value::Memory(_) => {
                        self.stack.push(val);
                    }
                    _ if array_wrapper_value_to_array_value(&val, &self.struct_heap)?.is_some() => {
                        self.stack.push(val);
                    }
                    Value::Tuple(_) => {
                        self.stack.push(val);
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "values: expected Dict, NamedTuple, Array, or Tuple, got {:?}",
                            crate::vm::util::value_type_name(&val)
                        )));
                    }
                }
            }

            BuiltinId::DictPairs => {
                if self.try_dispatch_struct_dict(&["pairs", "Base.pairs"], argc)? {
                    return Ok(Some(()));
                }
                if self.try_dispatch_struct_non_dict(&["pairs", "Base.pairs"], argc)? {
                    return Ok(Some(()));
                }
                // pairs(namedtuple/array/tuple/memory) — non-Dict collections (Issue #1872)
                let val = self.stack.pop_value()?;
                match val {
                    Value::NamedTuple(nt) => {
                        self.stack
                            .push(Value::Pairs(super::value::PairsValue::from_named_tuple(nt)));
                    }
                    _ if is_native_array_value(&val) => {
                        let Some(arr) = native_array_value_ref(&val) else {
                            return Err(VmError::TypeError("pairs: expected Array".into()));
                        };
                        let borrowed = arr.borrow();
                        let len = borrowed.len();
                        let pairs: Vec<Value> = (0..len)
                            .map(|i| {
                                let value = borrowed.get(&[i as i64 + 1]).unwrap_or(Value::Nothing);
                                Value::Tuple(TupleValue {
                                    elements: vec![Value::I64(i as i64 + 1), value],
                                })
                            })
                            .collect();
                        self.stack
                            .push(Value::Tuple(TupleValue { elements: pairs }));
                    }
                    Value::Memory(mem) => {
                        let borrowed = mem.borrow();
                        let len = borrowed.len();
                        let pairs: Vec<Value> = (0..len)
                            .map(|i| {
                                let value = borrowed.data.get_value(i).unwrap_or(Value::Nothing);
                                Value::Tuple(TupleValue {
                                    elements: vec![Value::I64(i as i64 + 1), value],
                                })
                            })
                            .collect();
                        self.stack
                            .push(Value::Tuple(TupleValue { elements: pairs }));
                    }
                    _ if array_wrapper_value_to_array_value(&val, &self.struct_heap)?.is_some() => {
                        let arr = array_wrapper_value_to_array_value(&val, &self.struct_heap)?
                            .ok_or_else(|| VmError::TypeError("pairs: expected Array".into()))?;
                        let len = arr.len();
                        let pairs: Vec<Value> = (0..len)
                            .map(|i| {
                                let value = arr.get(&[i as i64 + 1]).unwrap_or(Value::Nothing);
                                Value::Tuple(TupleValue {
                                    elements: vec![Value::I64(i as i64 + 1), value],
                                })
                            })
                            .collect();
                        self.stack
                            .push(Value::Tuple(TupleValue { elements: pairs }));
                    }
                    Value::Tuple(t) => {
                        let pairs: Vec<Value> = t
                            .elements
                            .iter()
                            .enumerate()
                            .map(|(i, v)| {
                                Value::Tuple(TupleValue {
                                    elements: vec![Value::I64(i as i64 + 1), v.clone()],
                                })
                            })
                            .collect();
                        self.stack
                            .push(Value::Tuple(TupleValue { elements: pairs }));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "pairs: expected Dict, NamedTuple, Array, or Tuple, got {:?}",
                            crate::vm::util::value_type_name(&val)
                        )));
                    }
                }
            }

            BuiltinId::DictGetBang => {
                if self.try_dispatch_struct_dict(&["get!", "Base.get!"], argc)? {
                    return Ok(Some(()));
                }
                if self.try_dispatch_struct_non_dict(&["get!", "Base.get!"], argc)? {
                    return Ok(Some(()));
                }
                return Err(self.dict_no_method("get!", argc));
            }

            BuiltinId::DictMergeBang => {
                if self.try_dispatch_struct_dict(&["merge!", "Base.merge!"], argc)? {
                    return Ok(Some(()));
                }
                if self.try_dispatch_struct_non_dict(&["merge!", "Base.merge!"], argc)? {
                    return Ok(Some(()));
                }
                return Err(self.dict_no_method("merge!", argc));
            }

            BuiltinId::DictEmpty => {
                if self.try_dispatch_struct_dict(&["empty!", "Base.empty!"], argc)? {
                    return Ok(Some(()));
                }
                if self.try_dispatch_struct_non_dict(&["empty!", "Base.empty!"], argc)? {
                    return Ok(Some(()));
                }
                return Err(self.dict_no_method("empty!", argc));
            }

            BuiltinId::DictPop => {
                // pop!(dict, key[, default]) — pure Dict{K,V} dispatch (Issue #2748).
                // The compiler emits LoadDict + CallBuiltin + Swap + StoreDict and
                // expects [modified_dict, popped_val] after the call, while the
                // method returns ONE value. Push the dict ref before the call so
                // the post-return stack is [dict, return_val].
                let stack_len = self.stack.len();
                if stack_len >= argc && argc > 0 {
                    let dict_pos = stack_len - argc;
                    let is_struct_dict = match &self.stack[dict_pos] {
                        Value::StructRef(idx) => self
                            .struct_heap
                            .get(*idx)
                            .map(|s| {
                                &*s.struct_name == "Dict" || s.struct_name.starts_with("Dict{")
                            })
                            .unwrap_or(false),
                        _ => false,
                    };
                    if is_struct_dict {
                        let dict_ref = self.stack[dict_pos].clone();
                        let mut args = Vec::with_capacity(argc);
                        for _ in 0..argc {
                            args.push(self.stack.pop_value()?);
                        }
                        args.reverse();
                        if let Some(func_index) =
                            self.find_best_method_index(&["pop!", "Base.pop!"], &args)
                        {
                            self.stack.push(dict_ref);
                            self.start_function_call(func_index, args)?;
                            return Ok(Some(()));
                        }
                        let type_name = self.get_type_name(&args[0]);
                        return Err(VmError::MethodError(format!(
                            "no method matching pop!({})",
                            type_name
                        )));
                    }
                }
                return Err(self.dict_no_method("pop!", argc));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
