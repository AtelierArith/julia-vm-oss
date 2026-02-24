//! Dict builtin functions for the VM.
//!
//! Dictionary operations: get, set, delete, keys, values, etc.
//! Also supports keys/values/pairs for arrays and tuples (Issue #1872).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::value::{DictKey, RangeValue, TupleValue, Value};
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
                .map(|s| s.struct_name == "Dict" || s.struct_name.starts_with("Dict{"))
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
        let is_non_dict_struct = match &self.stack[first_arg_pos] {
            Value::StructRef(idx) => self
                .struct_heap
                .get(*idx)
                .map(|s| s.struct_name != "Dict" && !s.struct_name.starts_with("Dict{"))
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

    /// Execute Dict builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a Dict builtin.
    pub(super) fn execute_builtin_dicts(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::DictGet => {
                // StructRef Dict dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["get", "Base.get"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch: e.g. get(::IOContext, key, default) (Issue #3152)
                if self.try_dispatch_struct_non_dict(&["get", "Base.get"], argc)? {
                    return Ok(Some(()));
                }
                // get(dict, key) or get(dict, key, default)
                if argc == 2 {
                    // get(dict, key) - error if key not found
                    let key_val = self.stack.pop_value()?;
                    let key = DictKey::from_value(&key_val)?;
                    let dict = self.stack.pop_dict()?;
                    match dict.get(&key) {
                        Some(val) => self.stack.push(val.clone()),
                        None => return Err(VmError::DictKeyNotFound(format!("{}", key))),
                    }
                } else if argc == 3 {
                    // get(dict, key, default) - return default if key not found
                    let default = self.stack.pop_value()?;
                    let key_val = self.stack.pop_value()?;
                    let key = DictKey::from_value(&key_val)?;
                    let dict = self.stack.pop_dict()?;
                    match dict.get(&key) {
                        Some(val) => self.stack.push(val.clone()),
                        None => self.stack.push(default),
                    }
                } else {
                    return Err(VmError::TypeError(format!(
                        "get requires 2 or 3 arguments, got {}",
                        argc
                    )));
                }
            }

            BuiltinId::DictGetkey => {
                // StructRef Dict dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["getkey", "Base.getkey"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch: e.g. getkey(::CustomContainer, key, default) (Issue #3169)
                if self.try_dispatch_struct_non_dict(&["getkey", "Base.getkey"], argc)? {
                    return Ok(Some(()));
                }
                // getkey(dict, key, default) - return the key if it exists, else default
                // This is useful for key canonicalization
                if argc != 3 {
                    return Err(VmError::TypeError(format!(
                        "getkey requires 3 arguments: getkey(dict, key, default), got {}",
                        argc
                    )));
                }
                let default = self.stack.pop_value()?;
                let key_val = self.stack.pop_value()?;
                let key = DictKey::from_value(&key_val)?;
                let dict = self.stack.pop_dict()?;
                if dict.contains_key(&key) {
                    // Return the key itself (as a Value)
                    self.stack.push(key_val);
                } else {
                    self.stack.push(default);
                }
            }

            BuiltinId::DictSet => {
                // StructRef dispatch for setindex! (Issue #2748, #3169)
                // Compilation order: push collection (args[0]), push key (args[2]), push value (args[1])
                // Stack: [collection (bottom), key (middle), value (top)]
                // Julia convention: setindex!(collection, value, key)
                // After pop+reverse we get [collection, key, value]; must swap args[1]↔args[2].
                {
                    let stack_len = self.stack.len();
                    if stack_len >= argc {
                        let first_arg_pos = stack_len - argc;
                        if matches!(&self.stack[first_arg_pos], Value::StructRef(_)) {
                            let mut args = Vec::with_capacity(argc);
                            for _ in 0..argc {
                                args.push(self.stack.pop_value()?);
                            }
                            args.reverse();
                            // Stack had [collection, key, value]; after reverse args = [collection, key, value].
                            // Swap to Julia convention: setindex!(collection, value, key).
                            if args.len() >= 3 {
                                args.swap(1, 2);
                            }
                            let func_names = &["setindex!", "Base.setindex!"];
                            if let Some(func_index) =
                                self.find_best_method_index(func_names, &args)
                            {
                                self.start_function_call(func_index, args)?;
                                return Ok(Some(()));
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching {}({})",
                                func_names[0], type_name
                            )));
                        }
                    }
                }
                // setindex!(dict, value, key) or dict[key] = value
                // Stack order: [dict (bottom), key (middle), value (top)]
                // Pop value first (top), then key, then dict.
                let value = self.stack.pop_value()?;
                let key_raw = self.stack.pop_value()?;
                let key = DictKey::from_value(&key_raw)?;
                let mut dict = self.stack.pop_dict()?;
                dict.insert(key, value);
                self.stack.push(Value::Dict(dict));
            }

            BuiltinId::DictDelete => {
                // StructRef Dict dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["delete!", "Base.delete!"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch: e.g. delete!(::CustomContainer, key) (Issue #3169)
                if self.try_dispatch_struct_non_dict(&["delete!", "Base.delete!"], argc)? {
                    return Ok(Some(()));
                }
                // delete!(dict_or_set, key)
                let key_val = self.stack.pop_value()?;
                let key = DictKey::from_value(&key_val)?;
                let collection = self.stack.pop_value()?;
                match collection {
                    Value::Dict(mut dict) => {
                        dict.remove(&key);
                        self.stack.push(Value::Dict(dict));
                    }
                    Value::Set(mut set) => {
                        set.remove(&key);
                        self.stack.push(Value::Set(set));
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "delete! requires Dict or Set, got {:?}",
                            crate::vm::util::value_type_name(&other)
                        )));
                    }
                }
            }

            BuiltinId::DictHasKey => {
                // StructRef Dict dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["haskey", "Base.haskey"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch: e.g. haskey(::IOContext, key) (Issue #3152)
                if self.try_dispatch_struct_non_dict(&["haskey", "Base.haskey"], argc)? {
                    return Ok(Some(()));
                }
                // haskey(dict, key) -> Bool
                let key_val = self.stack.pop_value()?;
                let key = DictKey::from_value(&key_val)?;
                let dict = self.stack.pop_dict()?;
                let has = dict.contains_key(&key);
                self.stack.push(Value::Bool(has));
            }

            BuiltinId::DictLen => {
                // StructRef Dict dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["length", "Base.length"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch: e.g. length(::CustomContainer) (Issue #3169)
                if self.try_dispatch_struct_non_dict(&["length", "Base.length"], argc)? {
                    return Ok(Some(()));
                }
                // length(dict) -> Int64
                let dict = self.stack.pop_dict()?;
                self.stack.push(Value::I64(dict.len() as i64));
            }

            BuiltinId::DictKeys => {
                // StructRef Dict dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["keys", "Base.keys"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch (Issue #3169)
                if self.try_dispatch_struct_non_dict(&["keys", "Base.keys"], argc)? {
                    return Ok(Some(()));
                }
                // keys(dict) -> Tuple of keys
                // Also supports keys(namedtuple) -> Tuple of symbol keys
                // Also supports keys(array) -> 1:length(array) (Issue #1872)
                // Also supports keys(tuple) -> 1:length(tuple) (Issue #1872)
                let val = self.stack.pop_value()?;
                match val {
                    Value::Dict(dict) => {
                        let keys: Vec<Value> =
                            dict.keys().into_iter().map(|k| k.to_value()).collect();
                        self.stack.push(Value::Tuple(TupleValue { elements: keys }));
                    }
                    Value::NamedTuple(nt) => {
                        // Return keys as tuple of symbols
                        let keys: Vec<Value> = nt
                            .names
                            .iter()
                            .map(|n| Value::Symbol(super::value::SymbolValue::new(n)))
                            .collect();
                        self.stack.push(Value::Tuple(TupleValue { elements: keys }));
                    }
                    Value::Array(arr) => {
                        // keys(array) returns 1:length(array)
                        let len = arr.borrow().len();
                        self.stack
                            .push(Value::Range(RangeValue::unit_range(1.0, len as f64)));
                    }
                    // Memory → Array (Issue #2764)
                    Value::Memory(mem) => {
                        let arr = super::util::memory_to_array_ref(&mem);
                        let len = arr.borrow().len();
                        self.stack
                            .push(Value::Range(RangeValue::unit_range(1.0, len as f64)));
                    }
                    Value::Tuple(t) => {
                        // keys(tuple) returns 1:length(tuple)
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
                // StructRef Dict dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["values", "Base.values"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch (Issue #3169)
                if self.try_dispatch_struct_non_dict(&["values", "Base.values"], argc)? {
                    return Ok(Some(()));
                }
                // values(dict) -> Tuple of values
                // Also supports values(namedtuple) -> Tuple of values
                // Also supports values(array) -> array itself (Issue #1872)
                // Also supports values(tuple) -> tuple itself (Issue #1872)
                let val = self.stack.pop_value()?;
                match val {
                    Value::Dict(dict) => {
                        let values: Vec<Value> = dict.values().into_iter().collect();
                        self.stack
                            .push(Value::Tuple(TupleValue { elements: values }));
                    }
                    Value::NamedTuple(nt) => {
                        self.stack.push(Value::Tuple(TupleValue {
                            elements: nt.values,
                        }));
                    }
                    Value::Array(_) => {
                        // values(array) returns the array itself
                        self.stack.push(val);
                    }
                    // Memory → Array (Issue #2764)
                    Value::Memory(_) => {
                        // values(memory) returns the memory itself
                        self.stack.push(val);
                    }
                    Value::Tuple(_) => {
                        // values(tuple) returns the tuple itself
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
                // StructRef Dict dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["pairs", "Base.pairs"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch (Issue #3169)
                if self.try_dispatch_struct_non_dict(&["pairs", "Base.pairs"], argc)? {
                    return Ok(Some(()));
                }
                // pairs(dict) -> Tuple of (key, value) tuples
                // Also supports pairs(namedtuple) -> Tuple of (symbol, value) tuples
                // Also supports pairs(array) -> Tuple of (index, value) tuples (Issue #1872)
                // Also supports pairs(tuple) -> Tuple of (index, value) tuples (Issue #1872)
                let val = self.stack.pop_value()?;
                match val {
                    Value::Dict(dict) => {
                        let pairs: Vec<Value> = dict
                            .iter()
                            .map(|(k, v)| {
                                Value::Tuple(TupleValue {
                                    elements: vec![k.to_value(), v.clone()],
                                })
                            })
                            .collect();
                        self.stack
                            .push(Value::Tuple(TupleValue { elements: pairs }));
                    }
                    Value::NamedTuple(nt) => {
                        let pairs: Vec<Value> = nt
                            .names
                            .iter()
                            .zip(nt.values.iter())
                            .map(|(name, val)| {
                                Value::Tuple(TupleValue {
                                    elements: vec![
                                        Value::Symbol(super::value::SymbolValue::new(name)),
                                        val.clone(),
                                    ],
                                })
                            })
                            .collect();
                        self.stack
                            .push(Value::Tuple(TupleValue { elements: pairs }));
                    }
                    Value::Array(arr) => {
                        // pairs(array) returns (index, value) pairs (1-indexed)
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
                    // Memory → Array (Issue #2764)
                    Value::Memory(mem) => {
                        // pairs(memory) returns (index, value) pairs (1-indexed)
                        let arr = super::util::memory_to_array_ref(&mem);
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
                    Value::Tuple(t) => {
                        // pairs(tuple) returns (index, value) pairs (1-indexed)
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

            BuiltinId::DictMerge => {
                // StructRef Dict dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["merge", "Base.merge"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch (Issue #3169)
                if self.try_dispatch_struct_non_dict(&["merge", "Base.merge"], argc)? {
                    return Ok(Some(()));
                }
                // merge(dict1, dict2) -> merged dict
                let dict2 = self.stack.pop_dict()?;
                let mut dict1 = self.stack.pop_dict()?;
                for (k, v) in dict2.iter() {
                    dict1.insert(k.clone(), v.clone());
                }
                self.stack.push(Value::Dict(dict1));
            }

            BuiltinId::DictNew => {
                // Dict() - create empty dict
                self.stack.push(Value::Dict(Box::default()));
            }

            BuiltinId::DictGetBang => {
                // StructRef Dict dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["get!", "Base.get!"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch (Issue #3169)
                if self.try_dispatch_struct_non_dict(&["get!", "Base.get!"], argc)? {
                    return Ok(Some(()));
                }
                // get!(dict, key, default) - get value or insert default and return it
                if argc != 3 {
                    return Err(VmError::TypeError(format!(
                        "get! requires 3 arguments, got {}",
                        argc
                    )));
                }
                let default = self.stack.pop_value()?;
                let key_val = self.stack.pop_value()?;
                let key = DictKey::from_value(&key_val)?;
                let mut dict = self.stack.pop_dict()?;

                let result = if let Some(existing) = dict.get(&key) {
                    existing.clone()
                } else {
                    dict.insert(key, default.clone());
                    default
                };
                self.stack.push(Value::Dict(dict));
                self.stack.push(result);
            }

            BuiltinId::DictMergeBang => {
                // StructRef Dict dispatch (Issue #2748)
                if self.try_dispatch_struct_dict(&["merge!", "Base.merge!"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch (Issue #3169)
                if self.try_dispatch_struct_non_dict(&["merge!", "Base.merge!"], argc)? {
                    return Ok(Some(()));
                }
                // merge!(dict1, dict2) - merge dict2 into dict1 in-place
                if argc != 2 {
                    return Err(VmError::TypeError(format!(
                        "merge! requires 2 arguments, got {}",
                        argc
                    )));
                }
                let dict2 = self.stack.pop_dict()?;
                let mut dict1 = self.stack.pop_dict()?;
                for (k, v) in dict2.iter() {
                    dict1.insert(k.clone(), v.clone());
                }
                self.stack.push(Value::Dict(dict1));
            }

            BuiltinId::DictEmpty => {
                // empty!(dict) - remove all entries from dict
                if self.try_dispatch_struct_dict(&["empty!", "Base.empty!"], argc)? {
                    return Ok(Some(()));
                }
                // Non-Dict StructRef dispatch (Issue #3169)
                if self.try_dispatch_struct_non_dict(&["empty!", "Base.empty!"], argc)? {
                    return Ok(Some(()));
                }
                if argc != 1 {
                    return Err(VmError::TypeError(format!(
                        "empty! requires 1 argument, got {}",
                        argc
                    )));
                }
                let mut dict = self.stack.pop_dict()?;
                dict.clear();
                self.stack.push(Value::Dict(dict));
            }

            BuiltinId::DictPop => {
                // pop!(dict, key) or pop!(dict, key, default)
                // Removes and returns the value for key, or returns default if key not found
                //
                // Special StructRef Dict handling (Issue #2748):
                // The compiler generates: LoadDict + CallBuiltin + Swap + StoreDict,
                // expecting TWO values [modified_dict, popped_val] on stack after the call.
                // try_dispatch_struct_dict only produces ONE (the function's return value).
                // Fix: push the dict ref before starting the function call so after return
                // the stack is [dict_ref, return_val] as expected.
                {
                    let stack_len = self.stack.len();
                    if stack_len >= argc {
                        let dict_pos = stack_len - argc;
                        let is_struct_dict = match &self.stack[dict_pos] {
                            Value::StructRef(idx) => self
                                .struct_heap
                                .get(*idx)
                                .map(|s| {
                                    s.struct_name == "Dict"
                                        || s.struct_name.starts_with("Dict{")
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
                                // Push dict ref FIRST so stack is [dict, return_val] after call
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
                }
                if argc == 2 {
                    // pop!(dict, key) - error if key not found
                    let key_val = self.stack.pop_value()?;
                    let key = DictKey::from_value(&key_val)?;
                    let mut dict = self.stack.pop_dict()?;
                    match dict.remove(&key) {
                        Some(val) => {
                            self.stack.push(Value::Dict(dict));
                            self.stack.push(val);
                        }
                        None => return Err(VmError::DictKeyNotFound(format!("{}", key))),
                    }
                } else if argc == 3 {
                    // pop!(dict, key, default) - return default if key not found
                    let default = self.stack.pop_value()?;
                    let key_val = self.stack.pop_value()?;
                    let key = DictKey::from_value(&key_val)?;
                    let mut dict = self.stack.pop_dict()?;
                    let val = dict.remove(&key).unwrap_or(default);
                    self.stack.push(Value::Dict(dict));
                    self.stack.push(val);
                } else {
                    return Err(VmError::TypeError(format!(
                        "pop! for dict requires 2 or 3 arguments, got {}",
                        argc
                    )));
                }
            }

            // =================================================================
            // Internal Dict intrinsics (Issue #2572)
            // These are low-level intrinsics called by Pure Julia wrappers.
            // =================================================================
            BuiltinId::_DictGet => {
                // _dict_get(d, key) - HashMap lookup, error if not found
                let key_val = self.stack.pop_value()?;
                let key = DictKey::from_value(&key_val)?;
                let dict = self.stack.pop_dict()?;
                match dict.get(&key) {
                    Some(val) => self.stack.push(val.clone()),
                    None => return Err(VmError::DictKeyNotFound(format!("{}", key))),
                }
            }

            BuiltinId::_DictSet => {
                // _dict_set!(d, key, value) - HashMap insert
                let value = self.stack.pop_value()?;
                let key_val = self.stack.pop_value()?;
                let key = DictKey::from_value(&key_val)?;
                let mut dict = self.stack.pop_dict()?;
                dict.insert(key, value);
                self.stack.push(Value::Dict(dict));
            }

            BuiltinId::_DictDelete => {
                // _dict_delete!(d, key) - HashMap remove (supports Dict and Set)
                let key_val = self.stack.pop_value()?;
                let key = DictKey::from_value(&key_val)?;
                let collection = self.stack.pop_value()?;
                match collection {
                    Value::Dict(mut dict) => {
                        dict.remove(&key);
                        self.stack.push(Value::Dict(dict));
                    }
                    Value::Set(mut set) => {
                        set.remove(&key);
                        self.stack.push(Value::Set(set));
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_dict_delete! requires Dict or Set, got {:?}",
                            crate::vm::util::value_type_name(&other)
                        )));
                    }
                }
            }

            BuiltinId::_DictHaskey => {
                // _dict_haskey(d, key) - HashMap contains_key
                let key_val = self.stack.pop_value()?;
                let key = DictKey::from_value(&key_val)?;
                let dict = self.stack.pop_dict()?;
                let has = dict.contains_key(&key);
                self.stack.push(Value::Bool(has));
            }

            BuiltinId::_DictLength => {
                // _dict_length(d) - HashMap len
                let dict = self.stack.pop_dict()?;
                self.stack.push(Value::I64(dict.len() as i64));
            }

            BuiltinId::_DictEmpty => {
                // _dict_empty!(d) - HashMap clear
                let mut dict = self.stack.pop_dict()?;
                dict.clear();
                self.stack.push(Value::Dict(dict));
            }

            // =================================================================
            // Internal Dict intrinsics for Pure Julia keys/values/pairs (Issue #2669)
            // =================================================================
            BuiltinId::_DictKeys => {
                // _dict_keys(d) - return all keys as Tuple
                let dict = self.stack.pop_dict()?;
                let keys: Vec<Value> =
                    dict.keys().into_iter().map(|k| k.to_value()).collect();
                self.stack.push(Value::Tuple(TupleValue { elements: keys }));
            }

            BuiltinId::_DictValues => {
                // _dict_values(d) - return all values as Tuple
                let dict = self.stack.pop_dict()?;
                let values: Vec<Value> = dict.values().into_iter().collect();
                self.stack
                    .push(Value::Tuple(TupleValue { elements: values }));
            }

            BuiltinId::_DictPairs => {
                // _dict_pairs(d) - return all entries as Tuple of 2-element Tuples
                let dict = self.stack.pop_dict()?;
                let pairs: Vec<Value> = dict
                    .iter()
                    .map(|(k, v)| {
                        Value::Tuple(TupleValue {
                            elements: vec![k.to_value(), v.clone()],
                        })
                    })
                    .collect();
                self.stack
                    .push(Value::Tuple(TupleValue { elements: pairs }));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
