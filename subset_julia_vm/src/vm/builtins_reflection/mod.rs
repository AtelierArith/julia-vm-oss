//! Reflection builtin functions for the VM.
//!
//! Internal introspection operations: _fieldnames, _fieldtypes, deepcopy, methods, hasmethod, which.
//! These are internal VM builtins that are wrapped by Pure Julia functions
//! in subset_julia_vm/src/julia/base/reflection.jl.

// SAFETY: i64/i32→usize casts for field index access are guarded by `if index == 0`
// checks that reject non-positive values before the cast.
#![allow(clippy::cast_sign_loss)]

mod primitives;

use crate::builtins::BuiltinId;
use crate::rng::RngLike;
use crate::types::JuliaType;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::value::{new_array_ref, ArrayValue, StructInstance, SymbolValue, TupleValue, Value};
use super::{FunctionInfo, Vm};
use primitives::{extract_func_name, extract_types_from_value, value_type_to_julia_type};

impl<R: RngLike> Vm<R> {
    /// Execute reflection builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a reflection builtin.
    pub(super) fn execute_builtin_reflection(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::_Fieldnames => {
                // fieldnames(T) - tuple of field names as symbols/strings
                let val = self.stack.pop_value()?;
                let names: Vec<Value> = match &val {
                    Value::StructRef(idx) => {
                        if let Some(si) = self.struct_heap.get(*idx) {
                            if let Some(def) = self
                                .struct_def_name_index
                                .get(&si.struct_name)
                                .and_then(|&idx| self.struct_defs.get(idx))
                            {
                                def.fields
                                    .iter()
                                    .map(|(name, _)| Value::Str(name.clone()))
                                    .collect()
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        }
                    }
                    Value::Struct(si) => {
                        if let Some(def) = self
                            .struct_def_name_index
                            .get(&si.struct_name)
                            .and_then(|&idx| self.struct_defs.get(idx))
                        {
                            def.fields
                                .iter()
                                .map(|(name, _)| Value::Str(name.clone()))
                                .collect()
                        } else {
                            vec![]
                        }
                    }
                    Value::DataType(jt) => {
                        let type_name = jt.name();
                        // Check for built-in types first
                        match type_name.as_ref() {
                            "LineNumberNode" => vec![
                                Value::Symbol(SymbolValue::new("line")),
                                Value::Symbol(SymbolValue::new("file")),
                            ],
                            "Expr" => vec![
                                Value::Symbol(SymbolValue::new("head")),
                                Value::Symbol(SymbolValue::new("args")),
                            ],
                            "QuoteNode" => vec![Value::Symbol(SymbolValue::new("value"))],
                            "GlobalRef" => vec![
                                Value::Symbol(SymbolValue::new("mod")),
                                Value::Symbol(SymbolValue::new("name")),
                            ],
                            _ => {
                                if let Some(def) = self
                                    .struct_defs
                                    .iter()
                                    .find(|d| d.name == type_name.as_ref())
                                {
                                    def.fields
                                        .iter()
                                        .map(|(name, _)| Value::Str(name.clone()))
                                        .collect()
                                } else {
                                    vec![]
                                }
                            }
                        }
                    }
                    Value::NamedTuple(nt) => {
                        nt.names.iter().map(|n| Value::Str(n.clone())).collect()
                    }
                    // Handle type name passed as string (e.g., fieldnames(Person))
                    Value::Str(type_name) => {
                        // Check for built-in types first
                        match type_name.as_str() {
                            "LineNumberNode" => vec![
                                Value::Symbol(SymbolValue::new("line")),
                                Value::Symbol(SymbolValue::new("file")),
                            ],
                            "Expr" => vec![
                                Value::Symbol(SymbolValue::new("head")),
                                Value::Symbol(SymbolValue::new("args")),
                            ],
                            "QuoteNode" => vec![Value::Symbol(SymbolValue::new("value"))],
                            "GlobalRef" => vec![
                                Value::Symbol(SymbolValue::new("mod")),
                                Value::Symbol(SymbolValue::new("name")),
                            ],
                            _ => {
                                if let Some(def) = self
                                    .struct_def_name_index
                                    .get(type_name)
                                    .and_then(|&idx| self.struct_defs.get(idx))
                                {
                                    def.fields
                                        .iter()
                                        .map(|(name, _)| Value::Str(name.clone()))
                                        .collect()
                                } else {
                                    vec![]
                                }
                            }
                        }
                    }
                    // LineNumberNode has fields: line, file
                    Value::LineNumberNode(_) => vec![
                        Value::Symbol(SymbolValue::new("line")),
                        Value::Symbol(SymbolValue::new("file")),
                    ],
                    _ => vec![],
                };
                self.stack
                    .push(Value::Tuple(TupleValue { elements: names }));
            }

            BuiltinId::_Fieldtypes => {
                // _fieldtypes(T) - tuple of field types as DataType values
                let val = self.stack.pop_value()?;
                let types: Vec<Value> = match &val {
                    Value::StructRef(idx) => {
                        if let Some(si) = self.struct_heap.get(*idx) {
                            if let Some(def) = self
                                .struct_def_name_index
                                .get(&si.struct_name)
                                .and_then(|&idx| self.struct_defs.get(idx))
                            {
                                def.fields
                                    .iter()
                                    .map(|(_, field_type)| {
                                        Value::DataType(value_type_to_julia_type(
                                            field_type,
                                            &self.struct_defs,
                                        ))
                                    })
                                    .collect()
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        }
                    }
                    Value::Struct(si) => {
                        if let Some(def) = self
                            .struct_def_name_index
                            .get(&si.struct_name)
                            .and_then(|&idx| self.struct_defs.get(idx))
                        {
                            def.fields
                                .iter()
                                .map(|(_, field_type)| {
                                    Value::DataType(value_type_to_julia_type(
                                        field_type,
                                        &self.struct_defs,
                                    ))
                                })
                                .collect()
                        } else {
                            vec![]
                        }
                    }
                    Value::DataType(jt) => {
                        let type_name = jt.name();
                        // Check for built-in types first
                        match type_name.as_ref() {
                            "LineNumberNode" => vec![
                                Value::DataType(JuliaType::Int64),  // line
                                Value::DataType(JuliaType::Symbol), // file
                            ],
                            "Expr" => vec![
                                Value::DataType(JuliaType::Symbol), // head
                                Value::DataType(JuliaType::Array),  // args (Vector{Any})
                            ],
                            "QuoteNode" => vec![Value::DataType(JuliaType::Any)], // value
                            "GlobalRef" => vec![
                                Value::DataType(JuliaType::Module), // mod
                                Value::DataType(JuliaType::Symbol), // name
                            ],
                            _ => {
                                if let Some(def) = self
                                    .struct_defs
                                    .iter()
                                    .find(|d| d.name == type_name.as_ref())
                                {
                                    def.fields
                                        .iter()
                                        .map(|(_, field_type)| {
                                            Value::DataType(value_type_to_julia_type(
                                                field_type,
                                                &self.struct_defs,
                                            ))
                                        })
                                        .collect()
                                } else {
                                    vec![]
                                }
                            }
                        }
                    }
                    // Handle type name passed as string (e.g., fieldtypes(Person))
                    Value::Str(type_name) => {
                        // Check for built-in types first
                        match type_name.as_str() {
                            "LineNumberNode" => vec![
                                Value::DataType(JuliaType::Int64),
                                Value::DataType(JuliaType::Symbol),
                            ],
                            "Expr" => vec![
                                Value::DataType(JuliaType::Symbol),
                                Value::DataType(JuliaType::Array),
                            ],
                            "QuoteNode" => vec![Value::DataType(JuliaType::Any)],
                            "GlobalRef" => vec![
                                Value::DataType(JuliaType::Module),
                                Value::DataType(JuliaType::Symbol),
                            ],
                            _ => {
                                if let Some(def) = self
                                    .struct_def_name_index
                                    .get(type_name)
                                    .and_then(|&idx| self.struct_defs.get(idx))
                                {
                                    def.fields
                                        .iter()
                                        .map(|(_, field_type)| {
                                            Value::DataType(value_type_to_julia_type(
                                                field_type,
                                                &self.struct_defs,
                                            ))
                                        })
                                        .collect()
                                } else {
                                    vec![]
                                }
                            }
                        }
                    }
                    // LineNumberNode has types: Int64, Symbol
                    Value::LineNumberNode(_) => vec![
                        Value::DataType(JuliaType::Int64),
                        Value::DataType(JuliaType::Symbol),
                    ],
                    _ => vec![],
                };
                self.stack
                    .push(Value::Tuple(TupleValue { elements: types }));
            }

            BuiltinId::_Getfield => {
                // _getfield(x, i) - get field value by index (1-based, like Julia)
                let index_val = self.stack.pop_value()?;
                let obj_val = self.stack.pop_value()?;

                let index = match &index_val {
                    Value::I64(i) => *i as usize,
                    Value::I32(i) => *i as usize,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "_getfield index must be an integer, got {:?}",
                            index_val
                        )))
                    }
                };

                // Convert from 1-based to 0-based indexing
                if index == 0 {
                    return Err(VmError::FieldIndexOutOfBounds {
                        index: 0,
                        field_count: 0,
                    });
                }
                let field_idx = index - 1;

                let field_value = match &obj_val {
                    Value::StructRef(idx) => {
                        if let Some(si) = self.struct_heap.get(*idx) {
                            si.get_field(field_idx).cloned()
                        } else {
                            None
                        }
                    }
                    Value::Struct(si) => si.get_field(field_idx).cloned(),
                    Value::Tuple(t) => t.elements.get(field_idx).cloned(),
                    Value::NamedTuple(nt) => nt.values.get(field_idx).cloned(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "_getfield requires a struct or tuple, got {:?}",
                            obj_val
                        )))
                    }
                };

                match field_value {
                    Some(v) => self.stack.push(v),
                    None => {
                        let field_count = match &obj_val {
                            Value::StructRef(idx) => self
                                .struct_heap
                                .get(*idx)
                                .map(|s| s.values.len())
                                .unwrap_or(0),
                            Value::Struct(si) => si.values.len(),
                            Value::Tuple(t) => t.elements.len(),
                            Value::NamedTuple(nt) => nt.values.len(),
                            _ => 0,
                        };
                        return Err(VmError::FieldIndexOutOfBounds {
                            index: field_idx,
                            field_count,
                        });
                    }
                }
            }

            BuiltinId::Getfield => {
                // getfield(x, name) or getfield(x, i) - get field by name (Symbol) or index (Int)
                let field_arg = self.stack.pop_value()?;
                let obj_val = self.stack.pop_value()?;

                // Determine if field access is by name (Symbol) or by index (Int)
                match &field_arg {
                    Value::Symbol(sym) => {
                        // Access by field name (Symbol)
                        let field_name = sym.as_str();
                        let field_value = match &obj_val {
                            Value::StructRef(idx) => {
                                if let Some(si) = self.struct_heap.get(*idx) {
                                    // Look up field index by name from struct definition
                                    let type_id = si.type_id;
                                    if let Some(def) = self.struct_defs.get(type_id) {
                                        if let Some(field_idx) = def
                                            .fields
                                            .iter()
                                            .position(|(name, _)| name == field_name)
                                        {
                                            si.get_field(field_idx).cloned()
                                        } else {
                                            return Err(VmError::TypeError(format!(
                                                "type {} has no field {}",
                                                si.struct_name, field_name
                                            )));
                                        }
                                    } else {
                                        return Err(VmError::TypeError(format!(
                                            "struct definition not found for type_id {}",
                                            type_id
                                        )));
                                    }
                                } else {
                                    return Err(VmError::TypeError(format!(
                                        "invalid StructRef({})",
                                        idx
                                    )));
                                }
                            }
                            Value::Struct(si) => {
                                let type_id = si.type_id;
                                if let Some(def) = self.struct_defs.get(type_id) {
                                    if let Some(field_idx) =
                                        def.fields.iter().position(|(name, _)| name == field_name)
                                    {
                                        si.get_field(field_idx).cloned()
                                    } else {
                                        return Err(VmError::TypeError(format!(
                                            "type {} has no field {}",
                                            si.struct_name, field_name
                                        )));
                                    }
                                } else {
                                    return Err(VmError::TypeError(format!(
                                        "struct definition not found for type_id {}",
                                        type_id
                                    )));
                                }
                            }
                            Value::NamedTuple(nt) => nt.get_by_name(field_name).ok().cloned(),
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "getfield with Symbol requires a struct or NamedTuple, got {:?}",
                                    obj_val
                                )));
                            }
                        };

                        match field_value {
                            Some(v) => self.stack.push(v),
                            None => {
                                return Err(VmError::TypeError(format!(
                                    "type has no field {}",
                                    field_name
                                )));
                            }
                        }
                    }
                    Value::I64(i) => {
                        // Access by integer index (1-based)
                        let index = *i as usize;
                        if index == 0 {
                            return Err(VmError::FieldIndexOutOfBounds {
                                index: 0,
                                field_count: 0,
                            });
                        }
                        let field_idx = index - 1;

                        let field_value = match &obj_val {
                            Value::StructRef(idx) => {
                                if let Some(si) = self.struct_heap.get(*idx) {
                                    si.get_field(field_idx).cloned()
                                } else {
                                    None
                                }
                            }
                            Value::Struct(si) => si.get_field(field_idx).cloned(),
                            Value::Tuple(t) => t.elements.get(field_idx).cloned(),
                            Value::NamedTuple(nt) => nt.values.get(field_idx).cloned(),
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "getfield requires a struct or tuple, got {:?}",
                                    obj_val
                                )));
                            }
                        };

                        match field_value {
                            Some(v) => self.stack.push(v),
                            None => {
                                let field_count = match &obj_val {
                                    Value::StructRef(idx) => self
                                        .struct_heap
                                        .get(*idx)
                                        .map(|s| s.values.len())
                                        .unwrap_or(0),
                                    Value::Struct(si) => si.values.len(),
                                    Value::Tuple(t) => t.elements.len(),
                                    Value::NamedTuple(nt) => nt.values.len(),
                                    _ => 0,
                                };
                                return Err(VmError::FieldIndexOutOfBounds {
                                    index: field_idx,
                                    field_count,
                                });
                            }
                        }
                    }
                    Value::I32(i) => {
                        // Handle I32 index as well
                        let index = *i as usize;
                        if index == 0 {
                            return Err(VmError::FieldIndexOutOfBounds {
                                index: 0,
                                field_count: 0,
                            });
                        }
                        let field_idx = index - 1;

                        let field_value = match &obj_val {
                            Value::StructRef(idx) => {
                                if let Some(si) = self.struct_heap.get(*idx) {
                                    si.get_field(field_idx).cloned()
                                } else {
                                    None
                                }
                            }
                            Value::Struct(si) => si.get_field(field_idx).cloned(),
                            Value::Tuple(t) => t.elements.get(field_idx).cloned(),
                            Value::NamedTuple(nt) => nt.values.get(field_idx).cloned(),
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "getfield requires a struct or tuple, got {:?}",
                                    obj_val
                                )));
                            }
                        };

                        match field_value {
                            Some(v) => self.stack.push(v),
                            None => {
                                let field_count = match &obj_val {
                                    Value::StructRef(idx) => self
                                        .struct_heap
                                        .get(*idx)
                                        .map(|s| s.values.len())
                                        .unwrap_or(0),
                                    Value::Struct(si) => si.values.len(),
                                    Value::Tuple(t) => t.elements.len(),
                                    Value::NamedTuple(nt) => nt.values.len(),
                                    _ => 0,
                                };
                                return Err(VmError::FieldIndexOutOfBounds {
                                    index: field_idx,
                                    field_count,
                                });
                            }
                        }
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "getfield field argument must be a Symbol or Int, got {:?}",
                            field_arg
                        )));
                    }
                }
            }

            BuiltinId::Setfield => {
                // setfield!(x, name, v) or setfield!(x, i, v) - set field by name (Symbol) or index (Int)
                let value = self.stack.pop_value()?;
                let field_arg = self.stack.pop_value()?;
                let obj_val = self.stack.pop_value()?;

                // Determine field index from name (Symbol) or index (Int)
                let field_idx = match &field_arg {
                    Value::Symbol(sym) => {
                        // Access by field name (Symbol)
                        let field_name = sym.as_str();
                        let type_id = match &obj_val {
                            Value::StructRef(idx) => self.struct_heap.get(*idx).map(|s| s.type_id),
                            Value::Struct(si) => Some(si.type_id),
                            _ => None,
                        };
                        if let Some(tid) = type_id {
                            if let Some(def) = self.struct_defs.get(tid) {
                                def.fields
                                    .iter()
                                    .position(|(name, _)| name == field_name)
                                    .ok_or_else(|| {
                                        VmError::TypeError(format!(
                                            "type has no field {}",
                                            field_name
                                        ))
                                    })?
                            } else {
                                return Err(VmError::TypeError(format!(
                                    "struct definition not found for type_id {}",
                                    tid
                                )));
                            }
                        } else {
                            return Err(VmError::TypeError(
                                "setfield! requires a mutable struct".into(),
                            ));
                        }
                    }
                    Value::I64(i) => {
                        // Access by integer index (1-based)
                        let index = *i as usize;
                        if index == 0 {
                            return Err(VmError::FieldIndexOutOfBounds {
                                index: 0,
                                field_count: 0,
                            });
                        }
                        index - 1
                    }
                    Value::I32(i) => {
                        let index = *i as usize;
                        if index == 0 {
                            return Err(VmError::FieldIndexOutOfBounds {
                                index: 0,
                                field_count: 0,
                            });
                        }
                        index - 1
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "setfield! field argument must be a Symbol or Int, got {:?}",
                            field_arg
                        )));
                    }
                };

                // Perform the field assignment
                match obj_val {
                    Value::StructRef(idx) => {
                        // Get type_id from heap
                        let type_id = self.struct_heap.get(idx).map(|s| s.type_id).unwrap_or(0);

                        // Check if struct is mutable
                        let is_mutable = self
                            .struct_defs
                            .get(type_id)
                            .map(|def| def.is_mutable)
                            .unwrap_or(false);

                        if !is_mutable {
                            let struct_name = self
                                .struct_defs
                                .get(type_id)
                                .map(|def| def.name.clone())
                                .unwrap_or_else(|| "unknown".to_string());
                            return Err(VmError::ImmutableFieldAssign(struct_name));
                        }

                        // Modify struct in heap directly
                        if let Some(s) = self.struct_heap.get_mut(idx) {
                            s.set_field(field_idx, value.clone())?;
                        }
                        // Return the assigned value (Julia semantics)
                        self.stack.push(value);
                    }
                    Value::Struct(mut s) => {
                        // Check if struct is mutable
                        let is_mutable = self
                            .struct_defs
                            .get(s.type_id)
                            .map(|def| def.is_mutable)
                            .unwrap_or(false);

                        if !is_mutable {
                            let struct_name = self
                                .struct_defs
                                .get(s.type_id)
                                .map(|def| def.name.clone())
                                .unwrap_or_else(|| "unknown".to_string());
                            return Err(VmError::ImmutableFieldAssign(struct_name));
                        }

                        s.set_field(field_idx, value.clone())?;
                        // Allocate on heap for mutation tracking
                        self.struct_heap.push(s);
                        // Return the assigned value (Julia semantics)
                        self.stack.push(value);
                    }
                    _ => {
                        return Err(VmError::TypeError(
                            "setfield! requires a mutable struct".into(),
                        ));
                    }
                }
            }

            BuiltinId::Deepcopy => {
                // deepcopy(x) - recursive deep copy
                let val = self.stack.pop_value()?;
                let copied = self.deep_copy_value(&val)?;
                self.stack.push(copied);
            }

            BuiltinId::HasMethod => {
                // hasmethod(f, types) - check if a method exists for the given function and types
                let types_val = self.stack.pop_value()?;
                let func_val = self.stack.pop_value()?;

                let func_name = extract_func_name(&func_val)?;
                let arg_types = extract_types_from_value(&types_val)?;

                let has_match = self
                    .find_matching_methods(&func_name, Some(&arg_types))
                    .is_some();
                self.stack.push(Value::Bool(has_match));
            }

            BuiltinId::Which => {
                // which(f, types) - get the specific method that would be called
                let types_val = self.stack.pop_value()?;
                let func_val = self.stack.pop_value()?;

                let func_name = extract_func_name(&func_val)?;
                let arg_types = extract_types_from_value(&types_val)?;

                match self.find_matching_methods(&func_name, Some(&arg_types)) {
                    Some(methods) if !methods.is_empty() => {
                        // Return the best matching method (first in the sorted list)
                        let info = &methods[0];
                        let method_struct = self.create_method_struct(info)?;
                        self.stack.push(method_struct);
                    }
                    _ => {
                        let type_str = arg_types
                            .iter()
                            .map(|t| t.name().to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        return Err(VmError::MethodError(format!(
                            "no method matching {}({})",
                            func_name, type_str
                        )));
                    }
                }
            }

            BuiltinId::Methods => {
                // methods(f) — all methods for a function
                // methods(f, types) — type-filtered method lookup (Issue #3257)
                let arg_types = if argc == 2 {
                    // Two-argument form: pop types argument first (stack is LIFO)
                    let types_val = self.stack.pop_value()?;
                    Some(extract_types_from_value(&types_val)?)
                } else {
                    None
                };
                let func_val = self.stack.pop_value()?;
                let func_name = extract_func_name(&func_val)?;

                let methods = self.find_matching_methods(
                    &func_name,
                    arg_types.as_deref(),
                );
                let method_values: Vec<Value> = match methods {
                    Some(infos) => infos
                        .iter()
                        .map(|info| self.create_method_struct(info))
                        .collect::<Result<Vec<_>, _>>()?,
                    None => vec![],
                };

                self.stack
                    .push(Value::Array(new_array_ref(ArrayValue::any_vector(
                        method_values,
                    ))));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }

    /// Find methods matching the given function name and optionally argument types.
    /// Returns None if no methods found, otherwise returns a vector of matching FunctionInfo
    /// sorted by specificity (most specific first).
    fn find_matching_methods(
        &self,
        func_name: &str,
        arg_types: Option<&[JuliaType]>,
    ) -> Option<Vec<FunctionInfo>> {
        let mut matches: Vec<(FunctionInfo, u32)> = Vec::new();

        for info in &self.functions {
            if info.name != func_name {
                continue;
            }

            // If no type filter, include all methods for this function
            let types = match arg_types {
                None => {
                    let score: u32 = info
                        .param_julia_types
                        .iter()
                        .map(|ty| ty.specificity() as u32)
                        .sum();
                    matches.push((info.clone(), score));
                    continue;
                }
                Some(types) => types,
            };

            // Check arity (handle varargs)
            let arity_match = if let Some(vararg_idx) = info.vararg_param_index {
                if let Some(fixed_count) = info.vararg_fixed_count {
                    // Vararg{T, N}: exactly vararg_idx + N args (Issue #2525)
                    types.len() == vararg_idx + fixed_count
                } else {
                    types.len() >= vararg_idx
                }
            } else {
                info.param_julia_types.len() == types.len()
            };

            if !arity_match {
                continue;
            }

            // Check type compatibility
            let fixed_count = info
                .vararg_param_index
                .unwrap_or(info.param_julia_types.len());
            let all_match = info
                .param_julia_types
                .iter()
                .take(fixed_count)
                .zip(types.iter().take(fixed_count))
                .all(|(param_ty, arg_ty)| {
                    arg_ty.is_subtype_of_parametric(param_ty, &info.type_params)
                });

            if all_match {
                // Score by specificity, prefer non-varargs
                let score: u32 = info
                    .param_julia_types
                    .iter()
                    .take(fixed_count)
                    .map(|ty| ty.specificity() as u32)
                    .sum();
                let adjusted = if info.vararg_param_index.is_some() {
                    score.saturating_sub(1)
                } else {
                    score
                };

                matches.push((info.clone(), adjusted));
            }
        }

        if matches.is_empty() {
            return None;
        }

        // Sort by score (descending - higher score = more specific)
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        Some(matches.into_iter().map(|(info, _)| info).collect())
    }

    /// Create a Method struct value from FunctionInfo
    fn create_method_struct(&self, info: &FunctionInfo) -> Result<Value, VmError> {
        // Find Method struct type_id
        let method_type_id = self
            .struct_defs
            .iter()
            .position(|def| def.name == "Method")
            .ok_or_else(|| VmError::TypeError("Method struct not defined".into()))?;

        // Build signature tuple from param_julia_types
        let sig_values: Vec<Value> = info
            .param_julia_types
            .iter()
            .map(|jt| Value::DataType(jt.clone()))
            .collect();
        let sig = Value::Tuple(TupleValue::new(sig_values));

        let method_struct = StructInstance {
            type_id: method_type_id,
            struct_name: "Method".to_string(),
            values: vec![
                Value::Symbol(SymbolValue::new(&info.name)),
                sig,
                Value::I64(info.params.len() as i64),
            ],
        };

        Ok(Value::Struct(method_struct))
    }
}
