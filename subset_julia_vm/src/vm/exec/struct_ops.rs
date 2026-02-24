//! Struct operations for the VM.
//!
//! This module handles struct instructions:
//! - NewStruct, NewStructSplat, NewParametricStruct, NewDynamicParametricStruct
//! - LoadStruct, StoreStruct
//! - GetField, GetExprField, SetField
//! - ReturnStruct

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::util::{self, resolve_any_type_param};
use super::super::value::{StructInstance, SymbolValue, Value};
use super::super::frame::VarTypeTag;
use super::super::Vm;

const EXPR_FIELD_HEAD_INDEX: usize = 0;
const EXPR_FIELD_ARGS_INDEX: usize = 1;
const LINE_NUMBER_NODE_FIELD_LINE_INDEX: usize = 0;
const LINE_NUMBER_NODE_FIELD_FILE_INDEX: usize = 1;
const GLOBAL_REF_FIELD_MODULE_INDEX: usize = 0;
const GLOBAL_REF_FIELD_NAME_INDEX: usize = 1;

/// Result of executing a struct instruction.
pub(super) enum StructResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Error was raised and caught by handler, continue to next iteration
    Continue,
    /// Return with value (exit run loop)
    Return(Value),
}

impl<R: RngLike> Vm<R> {
    /// Execute struct instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_struct(&mut self, instr: &Instr) -> Result<StructResult, VmError> {
        match instr {
            Instr::NewStruct(type_id, field_count) => {
                // Get struct definition info
                let mut struct_name = self
                    .struct_defs
                    .get(*type_id)
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                let struct_def = self.struct_defs.get(*type_id).cloned();
                let actual_field_count = struct_def
                    .as_ref()
                    .map(|def| def.fields.len())
                    .unwrap_or(*field_count);

                let values = if *field_count == 0 && actual_field_count > 0 {
                    // Partial initialization: new() with no args
                    // Create struct with all fields set to Undef
                    vec![Value::Undef; actual_field_count]
                } else {
                    // Normal case: pop values from stack
                    let mut vals = Vec::with_capacity(*field_count);
                    for _ in 0..*field_count {
                        vals.push(self.stack.pop_value()?);
                    }
                    vals.reverse(); // Restore original order
                    vals
                };

                // If struct has {Any} type parameter, resolve it from actual runtime values
                if let Some(resolved_name) = resolve_any_type_param(&struct_name, &values) {
                    struct_name = resolved_name;
                }

                // Allocate struct on heap and push reference
                let idx = self.struct_heap.len();
                self.struct_heap
                    .push(StructInstance::with_name(*type_id, struct_name, values));
                self.stack.push(Value::StructRef(idx));
                Ok(StructResult::Handled)
            }

            Instr::NewStructSplat(type_id) => {
                // Pop a tuple/array and unpack its elements into struct fields
                let val = self.stack.pop_value()?;
                let values: Vec<Value> = match val {
                    Value::Tuple(t) => t.elements,
                    Value::Array(arr) => {
                        // Convert array elements to Values (works with any array type)
                        let arr_borrow = arr.borrow();
                        (0..arr_borrow.len())
                            .filter_map(|i| arr_borrow.data.get_value(i))
                            .collect()
                    }
                    Value::Memory(mem) => {
                        // Memory → Array conversion for struct splatting (Issue #2764)
                        let arr_ref = crate::vm::util::memory_to_array_ref(&mem);
                        let arr_borrow = arr_ref.borrow();
                        (0..arr_borrow.len())
                            .filter_map(|i| arr_borrow.data.get_value(i))
                            .collect()
                    }
                    _ => {
                        self.raise(VmError::type_error_expected(
                            "new(args...)",
                            "a tuple or array",
                            &val,
                        ))?;
                        vec![]
                    }
                };
                // Get struct name from struct_defs
                let mut struct_name = self
                    .struct_defs
                    .get(*type_id)
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                // If struct has {Any} type parameter, resolve it from actual runtime values
                if let Some(resolved_name) = resolve_any_type_param(&struct_name, &values) {
                    struct_name = resolved_name;
                }
                // Allocate struct on heap and push reference
                let idx = self.struct_heap.len();
                self.struct_heap
                    .push(StructInstance::with_name(*type_id, struct_name, values));
                self.stack.push(Value::StructRef(idx));
                Ok(StructResult::Handled)
            }

            Instr::NewParametricStruct(ref base_name, field_count) => {
                // Pop field values from stack
                let mut values = Vec::with_capacity(*field_count);
                for _ in 0..*field_count {
                    values.push(self.stack.pop_value()?);
                }
                values.reverse();

                // Construct struct name from base name and type bindings
                // e.g., "Rational" with T=Int64 becomes "Rational{Int64}"
                let struct_name = if let Some(frame) = self.frames.last() {
                    if !frame.type_bindings.is_empty() {
                        // Use type bindings to construct the parametric type name
                        let type_args: Vec<String> = frame
                            .type_bindings
                            .values()
                            .map(|jt| jt.name().to_string())
                            .collect();
                        format!("{}{{{}}}", base_name, type_args.join(", "))
                    } else {
                        // No type bindings - infer from field values
                        let type_arg = if let Some(first_val) = values.first() {
                            match first_val {
                                Value::I64(_) => "Int64",
                                Value::I32(_) => "Int32",
                                Value::I16(_) => "Int16",
                                Value::I8(_) => "Int8",
                                Value::I128(_) => "Int128",
                                Value::U64(_) => "UInt64",
                                Value::U32(_) => "UInt32",
                                Value::U16(_) => "UInt16",
                                Value::U8(_) => "UInt8",
                                Value::U128(_) => "UInt128",
                                Value::F64(_) => "Float64",
                                Value::F32(_) => "Float32",
                                Value::F16(_) => "Float16",
                                Value::Bool(_) => "Bool",
                                Value::BigInt(_) => "BigInt",
                                _ => "Any",
                            }
                        } else {
                            "Any"
                        };
                        format!("{}{{{}}}", base_name, type_arg)
                    }
                } else {
                    base_name.clone()
                };

                // Find or create the type_id for this instantiation
                let type_id = self
                    .struct_defs
                    .iter()
                    .position(|d| d.name == struct_name)
                    .unwrap_or(0);

                // Allocate struct on heap and push reference
                let idx = self.struct_heap.len();
                self.struct_heap
                    .push(StructInstance::with_name(type_id, struct_name, values));
                self.stack.push(Value::StructRef(idx));
                Ok(StructResult::Handled)
            }

            Instr::NewDynamicParametricStruct(ref base_name, field_count, type_param_count) => {
                // Pop type parameters first (they're on top of stack)
                let mut type_params = Vec::with_capacity(*type_param_count);
                for _ in 0..*type_param_count {
                    type_params.push(self.stack.pop_value()?);
                }
                type_params.reverse();

                // Pop field values
                let mut values = Vec::with_capacity(*field_count);
                for _ in 0..*field_count {
                    values.push(self.stack.pop_value()?);
                }
                values.reverse();

                // Construct struct name from base name and type parameters
                // Type parameters can be DataType values or Symbol values (for MIME{Symbol(...)} etc.)
                let type_args: Vec<String> = type_params
                    .iter()
                    .map(|v| match v {
                        Value::DataType(jt) => jt.name().to_string(),
                        Value::Symbol(sym) => format!("Symbol(\"{}\")", sym.as_str()),
                        _ => "Any".to_string(),
                    })
                    .collect();
                let struct_name = format!("{}{{{}}}", base_name, type_args.join(", "));

                // Find or create the type_id for this instantiation
                let type_id = self
                    .struct_defs
                    .iter()
                    .position(|d| d.name == struct_name)
                    .or_else(|| {
                        if *type_param_count == 0 {
                            return None;
                        }
                        let any_params = vec!["Any"; *type_param_count];
                        let fallback_name = format!("{}{{{}}}", base_name, any_params.join(", "));
                        self.struct_defs
                            .iter()
                            .position(|d| d.name == fallback_name)
                    })
                    .unwrap_or(0);

                // Allocate struct on heap and push reference
                let idx = self.struct_heap.len();
                self.struct_heap
                    .push(StructInstance::with_name(type_id, struct_name, values));
                self.stack.push(Value::StructRef(idx));
                Ok(StructResult::Handled)
            }

            Instr::ConstructParametricType(ref base_name, num_type_args) => {
                // Pop type arguments from stack (they're on top)
                let mut type_args = Vec::with_capacity(*num_type_args);
                for _ in 0..*num_type_args {
                    type_args.push(self.stack.pop_value()?);
                }
                type_args.reverse();

                // Convert type arguments to type name strings
                let type_arg_names: Vec<String> = type_args
                    .iter()
                    .map(|v| match v {
                        Value::DataType(jt) => jt.name().to_string(),
                        Value::Symbol(sym) => format!("Symbol(\"{}\")", sym.as_str()),
                        _ => "Any".to_string(),
                    })
                    .collect();

                // Construct the parametric type name: e.g., "Complex{Float64}"
                let type_name = if type_arg_names.is_empty() {
                    base_name.clone()
                } else {
                    format!("{}{{{}}}", base_name, type_arg_names.join(", "))
                };

                // Push the resulting DataType onto the stack
                use crate::types::JuliaType;
                self.stack
                    .push(Value::DataType(JuliaType::Struct(type_name)));
                Ok(StructResult::Handled)
            }

            Instr::LoadStruct(name) => {
                // Get heap index from current frame, fall back to global frame (frame 0)
                let val = self
                    .frames
                    .last()
                    .and_then(|frame| {
                        self.load_slot_value_by_name(frame, name).or_else(|| {
                            frame.locals_struct.get(name).copied().map(Value::StructRef)
                        })
                    })
                    .or_else(|| {
                        // Fall back to global frame for global struct variables
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                self.load_slot_value_by_name(frame, name).or_else(|| {
                                    frame.locals_struct.get(name).copied().map(Value::StructRef)
                                })
                            })
                        } else {
                            None
                        }
                    });
                match val {
                    Some(Value::StructRef(idx)) => self.stack.push(Value::StructRef(idx)),
                    Some(Value::Struct(s)) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        self.stack.push(Value::StructRef(idx));
                    }
                    _ => {
                        // Variable not found - raise error instead of creating invalid reference
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        return Ok(StructResult::Continue);
                    }
                }
                Ok(StructResult::Handled)
            }

            Instr::StoreStruct(name) => {
                let val = self.stack.pop_value()?;
                if let Some(frame) = self.frames.last_mut() {
                    match val {
                        Value::StructRef(idx) => {
                            // Store the heap index directly
                            frame.locals_struct.insert(name.clone(), idx);
                            frame.var_types.insert(name.clone(), VarTypeTag::Struct);
                        }
                        Value::Struct(s) => {
                            // Allocate on heap and store index
                            let idx = self.struct_heap.len();
                            self.struct_heap.push(s);
                            frame.locals_struct.insert(name.clone(), idx);
                            frame.var_types.insert(name.clone(), VarTypeTag::Struct);
                        }
                        other => {
                            return Err(VmError::type_error_expected(
                                "StoreStruct",
                                "struct",
                                &util::value_type_name(&other),
                            ));
                        }
                    }
                }
                Ok(StructResult::Handled)
            }

            Instr::GetField(field_idx) => {
                let val = self.stack.pop_value()?;
                let (field_value, field_count, _struct_name) = match &val {
                    Value::StructRef(idx) => {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            (
                                s.get_field(*field_idx).cloned(),
                                s.values.len(),
                                s.struct_name.clone(),
                            )
                        } else {
                            // INTERNAL: GetField StructRef index is compiler-generated; invalid ref means heap corruption
                            return Err(VmError::InternalError(format!(
                                "GetField: invalid StructRef({}), heap size: {}",
                                idx,
                                self.struct_heap.len()
                            )));
                        }
                    }
                    Value::Struct(s) => (
                        s.get_field(*field_idx).cloned(),
                        s.values.len(),
                        s.struct_name.clone(),
                    ),
                    other => {
                        return Err(VmError::type_error_expected(
                            "GetField",
                            "struct",
                            &util::value_type_name(other),
                        ));
                    }
                };
                let value = match field_value {
                    Some(v) => v,
                    None => {
                        self.raise(VmError::FieldIndexOutOfBounds {
                            index: *field_idx,
                            field_count,
                        })?;
                        return Ok(StructResult::Continue);
                    }
                };
                self.stack.push(value);
                Ok(StructResult::Handled)
            }

            Instr::GetFieldByName(field_name) => {
                let val = self.stack.pop_value()?;
                if let Value::NamedTuple(named) = &val {
                    let value = named.get_by_name(field_name).map_err(|e| {
                        VmError::TypeError(format!("NamedTuple has no field {}: {}", field_name, e))
                    })?;
                    self.stack.push(value.clone());
                    return Ok(StructResult::Handled);
                }

                // Handle RegexMatch field access
                if let Value::RegexMatch(m) = &val {
                    let value = match field_name.as_str() {
                        "match" => Value::Str(m.match_str.clone()),
                        "captures" => {
                            let elements: Vec<Value> = m
                                .captures
                                .iter()
                                .map(|c| match c {
                                    Some(s) => Value::Str(s.clone()),
                                    None => Value::Nothing,
                                })
                                .collect();
                            Value::Tuple(crate::vm::value::TupleValue::new(elements))
                        }
                        "offset" => Value::I64(m.offset),
                        "offsets" => {
                            let elements: Vec<Value> =
                                m.offsets.iter().map(|&o| Value::I64(o)).collect();
                            Value::Tuple(crate::vm::value::TupleValue::new(elements))
                        }
                        _ => {
                            // INTERNAL: GetFieldByName StructRef index is compiler-generated; invalid ref means heap corruption
                            return Err(VmError::InternalError(format!(
                                "type RegexMatch has no field {}",
                                field_name
                            )));
                        }
                    };
                    self.stack.push(value);
                    return Ok(StructResult::Handled);
                }

                let (struct_instance, struct_name) = match &val {
                    Value::StructRef(idx) => {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            (s.clone(), s.struct_name.clone())
                        } else {
                            // User-visible: user can access a nonexistent field on a RegexMatch value
                            return Err(VmError::TypeError(format!(
                                "GetFieldByName: invalid StructRef({}), heap size: {}",
                                idx,
                                self.struct_heap.len()
                            )));
                        }
                    }
                    Value::Struct(s) => (s.clone(), s.struct_name.clone()),
                    other => {
                        return Err(VmError::type_error_expected(
                            "GetFieldByName",
                            "struct",
                            &util::value_type_name(other),
                        ));
                    }
                };

                // Look up the struct definition to find field index by name
                let type_id = struct_instance.type_id;
                let field_idx = if let Some(def) = self.struct_defs.get(type_id) {
                    def.fields.iter().position(|(name, _)| name == field_name)
                } else {
                    None
                };

                // Fallback: if struct_defs lookup failed but this is a Complex struct,
                // resolve "re"/"im" fields directly (Complex always has re=0, im=1).
                // This handles Complex structs returned from interleaved array storage
                // where type_id may not match the runtime struct_defs ordering.
                let field_idx = field_idx.or_else(|| {
                    if struct_instance.is_complex() {
                        match field_name.as_str() {
                            "re" => Some(0),
                            "im" => Some(1),
                            _ => None,
                        }
                    } else {
                        // Try scanning all struct_defs to find correct definition by name
                        for def in &self.struct_defs {
                            if def.name == struct_name {
                                if let Some(pos) =
                                    def.fields.iter().position(|(name, _)| name == field_name)
                                {
                                    return Some(pos);
                                }
                            }
                        }
                        None
                    }
                });

                match field_idx {
                    Some(idx) => {
                        if let Some(value) = struct_instance.get_field(idx) {
                            self.stack.push(value.clone());
                        } else {
                            self.raise(VmError::FieldIndexOutOfBounds {
                                index: idx,
                                field_count: struct_instance.values.len(),
                            })?;
                            return Ok(StructResult::Continue);
                        }
                    }
                    None => {
                        // User-visible: user can access a nonexistent field on a struct type
                        return Err(VmError::TypeError(format!(
                            "type {} has no field {}",
                            struct_name, field_name
                        )));
                    }
                }
                Ok(StructResult::Handled)
            }

            Instr::GetExprField(field_idx) => {
                let val = self.stack.pop_value()?;
                let expr = match val {
                    Value::Expr(e) => e,
                    other => {
                        return Err(VmError::type_error_expected(
                            "GetExprField",
                            "Expr",
                            &util::value_type_name(&other),
                        ));
                    }
                };
                let field_val = match *field_idx {
                    EXPR_FIELD_HEAD_INDEX => Value::Symbol(expr.head.clone()),
                    EXPR_FIELD_ARGS_INDEX => expr.get_args(),
                    _ => {
                        // INTERNAL: GetExprField field index is compiler-generated; out-of-bounds is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "GetExprField: field index {} out of bounds (expected {} or {})",
                            field_idx, EXPR_FIELD_HEAD_INDEX, EXPR_FIELD_ARGS_INDEX
                        )));
                    }
                };
                self.stack.push(field_val);
                Ok(StructResult::Handled)
            }

            Instr::GetLineNumberNodeField(field_idx) => {
                let val = self.stack.pop_value()?;
                let ln = match val {
                    Value::LineNumberNode(ln) => ln,
                    other => {
                        return Err(VmError::type_error_expected(
                            "GetLineNumberNodeField",
                            "LineNumberNode",
                            &util::value_type_name(&other),
                        ));
                    }
                };
                let field_val = match *field_idx {
                    LINE_NUMBER_NODE_FIELD_LINE_INDEX => Value::I64(ln.line),
                    LINE_NUMBER_NODE_FIELD_FILE_INDEX => match ln.file {
                        Some(file) => Value::Symbol(SymbolValue::new(file)),
                        None => Value::Nothing,
                    },
                    _ => {
                        // INTERNAL: GetLineNumberNodeField field index is compiler-generated; out-of-bounds is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "GetLineNumberNodeField: field index {} out of bounds (expected {} or {})",
                            field_idx, LINE_NUMBER_NODE_FIELD_LINE_INDEX, LINE_NUMBER_NODE_FIELD_FILE_INDEX
                        )));
                    }
                };
                self.stack.push(field_val);
                Ok(StructResult::Handled)
            }

            Instr::GetQuoteNodeValue => {
                let val = self.stack.pop_value()?;
                let inner = match val {
                    Value::QuoteNode(inner) => *inner,
                    other => {
                        return Err(VmError::type_error_expected(
                            "GetQuoteNodeValue",
                            "QuoteNode",
                            &util::value_type_name(&other),
                        ));
                    }
                };
                self.stack.push(inner);
                Ok(StructResult::Handled)
            }

            Instr::GetGlobalRefField(field_idx) => {
                let val = self.stack.pop_value()?;
                let gr = match val {
                    Value::GlobalRef(gr) => gr,
                    other => {
                        return Err(VmError::type_error_expected(
                            "GetGlobalRefField",
                            "GlobalRef",
                            &util::value_type_name(&other),
                        ));
                    }
                };
                let field_val = match *field_idx {
                    GLOBAL_REF_FIELD_MODULE_INDEX => {
                        // .mod returns the Module (we create a Module value from the name)
                        use crate::vm::value::ModuleValue;
                        Value::Module(Box::new(ModuleValue::new(gr.module.clone())))
                    }
                    GLOBAL_REF_FIELD_NAME_INDEX => Value::Symbol(gr.name.clone()),
                    _ => {
                        // INTERNAL: GetGlobalRefField field index is compiler-generated; out-of-bounds is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "GetGlobalRefField: field index {} out of bounds (expected {} or {})",
                            field_idx, GLOBAL_REF_FIELD_MODULE_INDEX, GLOBAL_REF_FIELD_NAME_INDEX
                        )));
                    }
                };
                self.stack.push(field_val);
                Ok(StructResult::Handled)
            }

            Instr::SetField(field_idx) => {
                let value = self.stack.pop_value()?;
                let struct_val = self.stack.pop_value()?;

                match struct_val {
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
                            self.raise(VmError::ImmutableFieldAssign(struct_name))?;
                            return Ok(StructResult::Continue);
                        }

                        // Modify struct in heap directly
                        let set_result = if let Some(s) = self.struct_heap.get_mut(idx) {
                            s.set_field(*field_idx, value)
                        } else {
                            Ok(())
                        };
                        if self.try_or_handle(set_result)?.is_none() {
                            return Ok(StructResult::Continue);
                        }
                        // Push the same reference back
                        self.stack.push(Value::StructRef(idx));
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
                            self.raise(VmError::ImmutableFieldAssign(struct_name))?;
                            return Ok(StructResult::Continue);
                        }

                        if self
                            .try_or_handle(s.set_field(*field_idx, value))?
                            .is_none()
                        {
                            return Ok(StructResult::Continue);
                        }
                        // Allocate on heap and push reference
                        let new_idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        self.stack.push(Value::StructRef(new_idx));
                    }
                    other => {
                        return Err(VmError::type_error_expected(
                            "SetField",
                            "struct",
                            &util::value_type_name(&other),
                        ));
                    }
                }
                Ok(StructResult::Handled)
            }

            Instr::SetFieldByName(field_name) => {
                // Runtime field set by name - resolves correct field index at runtime
                // to avoid non-deterministic compile-time struct_table iteration.
                // (Issue #2748)
                let value = self.stack.pop_value()?;
                let struct_val = self.stack.pop_value()?;

                match struct_val {
                    Value::StructRef(idx) => {
                        let type_id = self.struct_heap.get(idx).map(|s| s.type_id).unwrap_or(0);

                        // Check mutability
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
                            self.raise(VmError::ImmutableFieldAssign(struct_name))?;
                            return Ok(StructResult::Continue);
                        }

                        // Look up field index by name at runtime
                        let field_idx = self.struct_defs.get(type_id).and_then(|def| {
                            def.fields.iter().position(|(name, _)| name == field_name)
                        });

                        // Fallback: scan struct_defs by struct name
                        let field_idx = field_idx.or_else(|| {
                            let struct_name = self
                                .struct_heap
                                .get(idx)
                                .map(|s| s.struct_name.clone())
                                .unwrap_or_default();
                            for def in &self.struct_defs {
                                if def.name == struct_name {
                                    if let Some(pos) =
                                        def.fields.iter().position(|(name, _)| name == field_name)
                                    {
                                        return Some(pos);
                                    }
                                }
                            }
                            None
                        });

                        match field_idx {
                            Some(fi) => {
                                let set_result = if let Some(s) = self.struct_heap.get_mut(idx) {
                                    s.set_field(fi, value)
                                } else {
                                    Ok(())
                                };
                                if self.try_or_handle(set_result)?.is_none() {
                                    return Ok(StructResult::Continue);
                                }
                                self.stack.push(Value::StructRef(idx));
                            }
                            None => {
                                // User-visible: user can attempt to set a nonexistent field on a mutable struct (StructRef path)
                                return Err(VmError::TypeError(format!(
                                    "SetFieldByName: no field '{}' on struct",
                                    field_name
                                )));
                            }
                        }
                    }
                    Value::Struct(mut s) => {
                        let type_id = s.type_id;

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
                            self.raise(VmError::ImmutableFieldAssign(struct_name))?;
                            return Ok(StructResult::Continue);
                        }

                        let field_idx = self.struct_defs.get(type_id).and_then(|def| {
                            def.fields.iter().position(|(name, _)| name == field_name)
                        });

                        // Fallback: scan by struct name
                        let field_idx = field_idx.or_else(|| {
                            for def in &self.struct_defs {
                                if def.name == s.struct_name {
                                    if let Some(pos) =
                                        def.fields.iter().position(|(name, _)| name == field_name)
                                    {
                                        return Some(pos);
                                    }
                                }
                            }
                            None
                        });

                        match field_idx {
                            Some(fi) => {
                                if self.try_or_handle(s.set_field(fi, value))?.is_none() {
                                    return Ok(StructResult::Continue);
                                }
                                let new_idx = self.struct_heap.len();
                                self.struct_heap.push(s);
                                self.stack.push(Value::StructRef(new_idx));
                            }
                            None => {
                                // User-visible: user can attempt to set a nonexistent field on a mutable struct (Struct path)
                                return Err(VmError::TypeError(format!(
                                    "SetFieldByName: no field '{}' on struct",
                                    field_name
                                )));
                            }
                        }
                    }
                    other => {
                        return Err(VmError::type_error_expected(
                            "SetFieldByName",
                            "struct",
                            &util::value_type_name(&other),
                        ));
                    }
                }
                Ok(StructResult::Handled)
            }

            Instr::ReturnStruct => {
                let val = self.stack.pop_value()?;
                if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    // Keep StructRef for internal returns
                    self.stack.push(val);
                    Ok(StructResult::Handled)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    // Convert StructRef to Struct for final return
                    match val {
                        Value::StructRef(idx) => {
                            if let Some(s) = self.struct_heap.get(idx) {
                                Ok(StructResult::Return(Value::Struct(s.clone())))
                            } else {
                                Ok(StructResult::Return(Value::Struct(StructInstance::new(
                                    0,
                                    Vec::new(),
                                ))))
                            }
                        }
                        Value::Struct(s) => Ok(StructResult::Return(Value::Struct(s))),
                        other => Err(VmError::type_error_expected(
                            "ReturnStruct",
                            "struct",
                            &util::value_type_name(&other),
                        )),
                    }
                }
            }

            _ => Ok(StructResult::NotHandled),
        }
    }
}
