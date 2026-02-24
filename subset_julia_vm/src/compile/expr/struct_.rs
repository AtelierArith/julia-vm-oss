//! Struct compilation (constructors and field access).

use crate::ir::core::Expr;
use crate::vm::{Instr, ValueType};

use super::super::{
    err, extract_module_path_from_expr, get_base_exported_constant_value, get_math_constant_value,
    is_stdlib_module, CResult, CoreCompiler, StructInfo,
};

const EXPR_FIELD_HEAD_INDEX: usize = 0;
const EXPR_FIELD_ARGS_INDEX: usize = 1;
const LINE_NUMBER_NODE_FIELD_LINE_INDEX: usize = 0;
const LINE_NUMBER_NODE_FIELD_FILE_INDEX: usize = 1;
const GLOBAL_REF_FIELD_MODULE_INDEX: usize = 0;
const GLOBAL_REF_FIELD_NAME_INDEX: usize = 1;

impl CoreCompiler<'_> {
    /// Compile a struct constructor call: Point(1.0, 2.0)
    pub(in super::super) fn compile_struct_constructor(
        &mut self,
        struct_info: StructInfo,
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Check that argument count matches field count
        if args.len() != struct_info.fields.len() {
            return err(format!(
                "Struct constructor expects {} arguments, got {}",
                struct_info.fields.len(),
                args.len()
            ));
        }

        // Compile each argument with the expected field type
        // For Any-typed fields and Function-typed fields, don't coerce - preserve the original type
        for (arg, (_, field_ty)) in args.iter().zip(struct_info.fields.iter()) {
            if *field_ty == ValueType::Any || *field_ty == ValueType::Function {
                // Any-typed or Function-typed fields: compile without type coercion
                // Function fields accept any callable (functions, composed functions, etc.)
                self.compile_expr(arg)?;
            } else {
                // Typed fields: compile with type coercion
                self.compile_expr_as(arg, field_ty.clone())?;
            }
        }

        // Emit NewStruct instruction
        self.emit(Instr::NewStruct(struct_info.type_id, args.len()));

        Ok(ValueType::Struct(struct_info.type_id))
    }

    /// Compile a field access: obj.field
    pub(in super::super) fn compile_field_access(
        &mut self,
        object: &Expr,
        field: &str,
    ) -> CResult<ValueType> {
        // Check for nested module path like Base.MathConstants.e
        if let Some(module_path) = extract_module_path_from_expr(object) {
            // Check if this is Base.MathConstants constant access
            if module_path == "Base.MathConstants" {
                if let Some(value) = get_math_constant_value(field) {
                    self.emit(Instr::PushF64(value));
                    return Ok(ValueType::F64);
                }
                return err(format!(
                    "Base.MathConstants has no constant named {}",
                    field
                ));
            }

            // Handle Base module constants (only pi, ℯ, Inf, NaN are exported from Base)
            // Other MathConstants like 'e', 'golden', 'eulergamma' require Base.MathConstants.e
            if module_path == "Base" {
                if let Some(value) = get_base_exported_constant_value(field) {
                    self.emit(Instr::PushF64(value));
                    return Ok(ValueType::F64);
                }
            }

            // Handle other Base submodules or module function refs
            if module_path.starts_with("Base.") || is_stdlib_module(&module_path) {
                return self.compile_module_function_ref(&module_path, field);
            }
        }

        if let Expr::Var(module_name, _) = object {
            let local_ty = self.locals.get(module_name).cloned();
            let is_module_value = matches!(local_ty, Some(ValueType::Module))
                || is_stdlib_module(module_name)
                || self.module_aliases.contains_key(module_name)
                || self.module_functions.contains_key(module_name);

            if is_module_value {
                let resolved_module = self
                    .module_aliases
                    .get(module_name)
                    .cloned()
                    .unwrap_or_else(|| module_name.clone());

                // Handle Base module constants (pi, e, Inf, NaN, etc.)
                // These are exported from Base.MathConstants but accessible as Base.pi
                if resolved_module == "Base" {
                    if let Some(value) = get_math_constant_value(field) {
                        self.emit(Instr::PushF64(value));
                        return Ok(ValueType::F64);
                    }
                }

                return self.compile_module_function_ref(&resolved_module, field);
            }
        }

        // Compile the object expression
        let obj_ty = self.compile_expr(object)?;

        match obj_ty {
            ValueType::Struct(type_id) => {
                // Look up the struct definition and find field info
                let mut result: Option<(usize, ValueType)> = None;
                let mut struct_name = String::new();

                for (name, struct_info) in self.shared_ctx.struct_table.iter() {
                    if struct_info.type_id == type_id {
                        struct_name = name.clone();
                        for (idx, (field_name, field_ty)) in struct_info.fields.iter().enumerate() {
                            if field_name == field {
                                result = Some((idx, field_ty.clone()));
                                break;
                            }
                        }
                        break;
                    }
                }

                match result {
                    Some((idx, field_ty)) => {
                        self.emit(Instr::GetField(idx));
                        Ok(field_ty)
                    }
                    None => {
                        if struct_name.is_empty() {
                            err(format!("Unknown struct type_id: {}", type_id))
                        } else {
                            err(format!(
                                "Unknown field '{}' on struct '{}'",
                                field, struct_name
                            ))
                        }
                    }
                }
            }
            ValueType::Any => {
                // For Any type, first check for special builtin type fields
                // Expr, LineNumberNode, GlobalRef have predefined fields that need runtime dispatch
                // These are checked before user-defined struct fields to support metaprogramming
                match field {
                    // Expr fields: head, args
                    "head" | "args" => {
                        // Emit dynamic field access that works for Expr at runtime
                        let field_idx = if field == "head" {
                            EXPR_FIELD_HEAD_INDEX
                        } else {
                            EXPR_FIELD_ARGS_INDEX
                        };
                        self.emit(Instr::GetExprField(field_idx));
                        return Ok(ValueType::Any);
                    }
                    _ => {}
                }

                // For user-defined structs, use runtime field lookup by name.
                // This is necessary because different structs may have the same field name
                // at different indices (e.g., DomainError.msg at index 1, DimensionMismatch.msg at index 0).
                // The GetFieldByName instruction looks up the field index at runtime using
                // the struct's type_id to find the correct definition.
                self.emit(Instr::GetFieldByName(field.to_string()));
                // Return Any since we don't know the concrete struct type at compile time.
                // The actual field type depends on the runtime struct instance.
                Ok(ValueType::Any)
            }
            // Expr type has special fields: head (Symbol) and args (Vector{Any})
            // This matches Julia's Core.Expr structure
            ValueType::Expr => {
                match field {
                    "head" => {
                        self.emit(Instr::GetExprField(EXPR_FIELD_HEAD_INDEX));
                        Ok(ValueType::Symbol)
                    }
                    "args" => {
                        self.emit(Instr::GetExprField(EXPR_FIELD_ARGS_INDEX));
                        Ok(ValueType::Array) // Vector{Any}
                    }
                    _ => err(format!("type Expr has no field {}", field)),
                }
            }
            // LineNumberNode type has special fields: line (Int64) and file (Symbol)
            // This matches Julia's LineNumberNode structure
            ValueType::LineNumberNode => {
                match field {
                    "line" => {
                        self.emit(Instr::GetLineNumberNodeField(
                            LINE_NUMBER_NODE_FIELD_LINE_INDEX,
                        ));
                        Ok(ValueType::I64)
                    }
                    "file" => {
                        self.emit(Instr::GetLineNumberNodeField(
                            LINE_NUMBER_NODE_FIELD_FILE_INDEX,
                        ));
                        Ok(ValueType::Symbol) // Returns Symbol (or nothing if no file)
                    }
                    _ => err(format!("type LineNumberNode has no field {}", field)),
                }
            }
            // QuoteNode type has special field: value (the wrapped value)
            // This matches Julia's QuoteNode structure
            ValueType::QuoteNode => {
                match field {
                    "value" => {
                        self.emit(Instr::GetQuoteNodeValue);
                        Ok(ValueType::Any) // The wrapped value can be any type
                    }
                    _ => err(format!("type QuoteNode has no field {}", field)),
                }
            }
            // GlobalRef type has special fields: mod (Module) and name (Symbol)
            // This matches Julia's GlobalRef structure
            ValueType::GlobalRef => match field {
                "mod" => {
                    self.emit(Instr::GetGlobalRefField(GLOBAL_REF_FIELD_MODULE_INDEX));
                    Ok(ValueType::Module)
                }
                "name" => {
                    self.emit(Instr::GetGlobalRefField(GLOBAL_REF_FIELD_NAME_INDEX));
                    Ok(ValueType::Symbol)
                }
                _ => err(format!("type GlobalRef has no field {}", field)),
            },
            // NamedTuple field access: nt.field
            // Julia supports both nt.field and nt[:field] for NamedTuples
            ValueType::NamedTuple => {
                self.emit(Instr::NamedTupleGetField(field.to_string()));
                Ok(ValueType::Any)
            }
            // Base.Pairs does NOT support dot notation - must use kwargs[:field]
            // This matches Julia's behavior where kwargs.field is an error
            ValueType::Pairs => err(format!(
                "type Base.Pairs has no field `{}`. Use kwargs[:{}] instead",
                field, field
            )),
            // For F64 and other types that might actually be structs at runtime
            // (e.g., when type inference couldn't determine the exact struct type),
            // check if any struct has this field and use runtime lookup
            _ => {
                // Check if any struct definition has this field name
                let mut found_field = false;

                // Search in instantiated structs
                for (_, struct_info) in self.shared_ctx.struct_table.iter() {
                    if struct_info.fields.iter().any(|(name, _)| name == field) {
                        found_field = true;
                        break;
                    }
                }

                // Also search in parametric struct definitions
                if !found_field {
                    for (_, param_def) in self.shared_ctx.parametric_structs.iter() {
                        if param_def.def.fields.iter().any(|f| f.name == field) {
                            found_field = true;
                            break;
                        }
                    }
                }

                if found_field {
                    // Use runtime field lookup by name since different structs may have
                    // the same field name at different indices.
                    self.emit(Instr::GetFieldByName(field.to_string()));
                    // Return Any because we don't know the actual struct type at compile time.
                    // The actual field type depends on the runtime struct instance.
                    Ok(ValueType::Any)
                } else {
                    err(format!(
                        "Field access requires a struct type, got {:?}",
                        obj_ty
                    ))
                }
            }
        }
    }
}
