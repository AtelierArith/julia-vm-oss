//! CoreCompiler struct definition and basic methods.
//!
//! This module defines the `CoreCompiler` struct which holds all state needed
//! during compilation of a function or module body, including:
//! - Emitted instructions
//! - Local variable type tracking
//! - Method tables for dispatch
//! - Loop/finally context stacks
//! - Closure capture tracking
//!
//! Supporting types `LoopContext` and `FinallyContext` are also defined here.

use std::collections::{HashMap, HashSet};

use crate::ir::core::Block;
use crate::types::{JuliaType, TypeParam};
use crate::vm::{Instr, ValueType};

use super::context::SharedCompileContext;
use super::method_table::MethodTable;
use super::type_helpers::julia_type_to_value_type;
use super::types::{self, CResult};

/// Loop context tracking patch points for break/continue
#[derive(Debug)]
pub(super) struct LoopContext {
    /// Instruction indices for loop exits (break)
    pub exit_patches: Vec<usize>,
    /// Instruction indices for loop continues
    pub continue_patches: Vec<usize>,
}

/// Finally block context for tracking pending finally blocks.
/// Used to ensure finally blocks execute even with return/break/continue.
#[derive(Debug)]
pub(super) struct FinallyContext {
    /// The finally block IR to execute
    pub finally_block: Block,
    /// Loop depth when this finally was pushed (for break/continue scoping)
    pub loop_depth: usize,
}

pub(super) struct CoreCompiler<'a> {
    pub(super) code: Vec<Instr>,
    pub(super) locals: HashMap<String, ValueType>,
    /// JuliaType tracking for parametric types that ValueType cannot represent.
    ///
    /// `ValueType::Tuple` is non-parametric — it only represents "some tuple" without
    /// element type information. When a tuple literal like `(42, 10)` is assigned to a
    /// variable, the `locals` map records it as `ValueType::Tuple`, losing the precise
    /// `Tuple{Int64, Int64}` type needed for parametric dispatch.
    ///
    /// This map stores the full `JuliaType` (e.g., `JuliaType::TupleOf([Int64, Int64])`)
    /// so that `infer_julia_type()` can recover precise parametric information when
    /// building argument type lists for method dispatch.
    ///
    /// **Lookup priority in `infer_julia_type()`:**
    /// 1. Check `julia_type_locals` first (parametric types like `Tuple{Int64, Int64}`)
    /// 2. Fall back to `locals` / `global_types` (ValueType-based inference)
    ///
    /// **Currently tracked:** Tuple literals assigned to variables (Issue #1748).
    /// Could be extended to other parametric types (NamedTuple, etc.) if needed.
    pub(super) julia_type_locals: HashMap<String, JuliaType>,
    pub(super) method_tables: &'a HashMap<String, MethodTable>,
    /// Module name -> Set of function names defined in that module
    pub(super) module_functions: &'a HashMap<String, HashSet<String>>,
    /// Module name -> Set of exported function names (empty = all exported)
    /// Kept to preserve compile-context contract while export handling evolves.
    #[allow(dead_code)]
    pub(super) module_exports: &'a HashMap<String, HashSet<String>>,
    /// Set of function names that are available via `using` (respects export + selective import)
    pub(super) imported_functions: &'a HashSet<String>,
    /// Set of module names imported via `using` (for backward compatibility)
    pub(super) usings: &'a HashSet<String>,
    pub(super) shared_ctx: &'a mut SharedCompileContext,
    pub(super) temp_counter: usize,
    /// Stack of active loops for break/continue support
    pub(super) loop_stack: Vec<LoopContext>,
    /// Stack of active finally blocks for return/break/continue handling
    pub(super) finally_stack: Vec<FinallyContext>,
    /// Whether we're in a function body (strict undefined var check) or module/main (lenient)
    pub(super) strict_undefined_check: bool,
    /// Parameters with Any type (no type annotation) - these preserve Any on reassignment
    pub(super) any_params: HashSet<String>,
    /// Parameters with abstract numeric type annotations (Number, Real, Integer, etc.)
    /// Binary operations on these must use runtime dispatch (Issue #2498)
    pub(super) abstract_numeric_params: HashSet<String>,
    /// Module aliases: variable name -> module name (e.g., "S" -> "Statistics")
    pub(super) module_aliases: HashMap<String, String>,
    /// Set of abstract type names (for isa() type checking)
    pub(super) abstract_type_names: &'a HashSet<String>,
    /// Current struct type_id for inner constructor compilation (for new() calls)
    pub(super) current_struct_type_id: Option<usize>,
    /// Current parametric struct base name (e.g., "Rational") for new{T}() calls
    pub(super) current_parametric_struct_name: Option<String>,
    /// Type parameters from current function's where clause (for type binding)
    pub(super) current_type_params: Vec<TypeParam>,
    /// Type param name -> index lookup (Issue #2865)
    pub(super) current_type_param_index: HashMap<String, usize>,
    /// Variables with mixed F64+I64 types - these need dynamic typing (StoreAny/LoadAny)
    pub(super) mixed_type_vars: HashSet<String>,
    /// Type parameters that come from Val{N} patterns - these should be I64, not DataType
    pub(super) val_type_params: HashSet<String>,
    /// Type parameters that come from Val{true}/Val{false} patterns - these should be Bool
    pub(super) val_bool_params: HashSet<String>,
    /// Type parameters that come from Val{:symbol} patterns - these should be Symbol
    pub(super) val_symbol_params: HashSet<String>,
    /// Current module path (e.g., "Dates") for resolving unqualified struct names
    pub(super) current_module_path: Option<String>,
    /// Module name -> Set of constant names defined in that module's body
    pub(super) module_constants: &'a HashMap<String, HashSet<String>>,
    /// Label positions: label_name -> instruction index (for @label)
    pub(super) label_positions: HashMap<String, usize>,
    /// Goto patches: (instruction_index, target_label_name) (for @goto)
    pub(super) goto_patches: Vec<(usize, String)>,
    /// Captured variables from outer scope (for closures).
    /// When compiling a closure body, this contains the names of variables
    /// that were captured from the enclosing function scope.
    pub(super) captured_vars: HashSet<String>,
    /// Current enclosing function name (for creating qualified nested function names).
    /// Used to disambiguate nested functions with the same name in different parent functions.
    pub(super) current_function_name: Option<String>,
}

/// Check if a ValueType is an integer type (signed or unsigned)
pub(super) fn is_integer_type(ty: &ValueType) -> bool {
    matches!(
        ty,
        ValueType::I64
            | ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::I128
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::U64
            | ValueType::U128
    )
}

/// Check if a ValueType is a floating-point type
pub(super) fn is_float_type(ty: &ValueType) -> bool {
    matches!(ty, ValueType::F64 | ValueType::F32 | ValueType::F16)
}

/// Check if a ValueType is any numeric type (integer or float)
pub(super) fn is_numeric_type(ty: &ValueType) -> bool {
    is_integer_type(ty) || is_float_type(ty)
}

/// Check if a ValueType is a singleton type.
///
/// Singleton types have identity semantics: equality (`==`) and identity (`===`)
/// are equivalent. When adding special handling for identity operators (`===`/`!==`),
/// always add corresponding handling for equality operators (`==`/`!=`).
///
/// SINGLETON_HANDLING: When modifying identity ops, update equality ops too.
pub(super) fn is_singleton_type(ty: &ValueType) -> bool {
    matches!(
        ty,
        ValueType::Nothing | ValueType::DataType | ValueType::Symbol | ValueType::Char
    )
}

impl<'a> CoreCompiler<'a> {
    pub(super) fn new(
        method_tables: &'a HashMap<String, MethodTable>,
        module_functions: &'a HashMap<String, HashSet<String>>,
        module_exports: &'a HashMap<String, HashSet<String>>,
        imported_functions: &'a HashSet<String>,
        usings: &'a HashSet<String>,
        shared_ctx: &'a mut SharedCompileContext,
        abstract_type_names: &'a HashSet<String>,
        module_constants: &'a HashMap<String, HashSet<String>>,
    ) -> Self {
        Self {
            code: Vec::with_capacity(64),
            locals: HashMap::new(),
            julia_type_locals: HashMap::new(),
            method_tables,
            module_functions,
            module_exports,
            imported_functions,
            usings,
            shared_ctx,
            temp_counter: 0,
            loop_stack: Vec::new(),
            finally_stack: Vec::new(),
            strict_undefined_check: false, // Default to lenient for module/main
            current_struct_type_id: None,
            current_parametric_struct_name: None,
            any_params: HashSet::new(), // No params in module/main context
            abstract_numeric_params: HashSet::new(),
            module_aliases: HashMap::new(),
            abstract_type_names,
            current_type_params: Vec::new(),
            current_type_param_index: HashMap::new(),
            mixed_type_vars: HashSet::new(), // No mixed type vars in module/main context
            val_type_params: HashSet::new(),
            val_bool_params: HashSet::new(),
            val_symbol_params: HashSet::new(),
            current_module_path: None,
            module_constants,
            label_positions: HashMap::new(),
            goto_patches: Vec::new(),
            captured_vars: HashSet::new(),
            current_function_name: None,
        }
    }

    pub(super) fn new_for_function(
        method_tables: &'a HashMap<String, MethodTable>,
        module_functions: &'a HashMap<String, HashSet<String>>,
        module_exports: &'a HashMap<String, HashSet<String>>,
        imported_functions: &'a HashSet<String>,
        usings: &'a HashSet<String>,
        shared_ctx: &'a mut SharedCompileContext,
        abstract_type_names: &'a HashSet<String>,
        module_constants: &'a HashMap<String, HashSet<String>>,
    ) -> Self {
        Self {
            code: Vec::with_capacity(64),
            locals: HashMap::new(),
            julia_type_locals: HashMap::new(),
            method_tables,
            module_functions,
            module_exports,
            imported_functions,
            usings,
            shared_ctx,
            temp_counter: 0,
            loop_stack: Vec::new(),
            finally_stack: Vec::new(),
            strict_undefined_check: true, // Strict for function bodies
            any_params: HashSet::new(),   // Will be populated after creation
            abstract_numeric_params: HashSet::new(), // Will be populated after creation
            module_aliases: HashMap::new(),
            abstract_type_names,
            current_struct_type_id: None,
            current_parametric_struct_name: None,
            current_type_params: Vec::new(), // Will be set after creation
            current_type_param_index: HashMap::new(), // Will be set after creation
            mixed_type_vars: HashSet::new(), // Will be populated from type inference
            val_type_params: HashSet::new(), // Will be populated from parameter analysis
            val_bool_params: HashSet::new(), // Will be populated from parameter analysis
            val_symbol_params: HashSet::new(), // Will be populated from parameter analysis
            current_module_path: None,       // Will be set after creation
            module_constants,
            label_positions: HashMap::new(),
            goto_patches: Vec::new(),
            captured_vars: HashSet::new(), // Will be populated for closures
            current_function_name: None,   // Will be set when compiling functions
        }
    }

    /// Convert JuliaType to ValueType, resolving struct types using the struct table.
    pub(super) fn julia_type_to_value_type_with_ctx(&self, jt: &JuliaType) -> ValueType {
        match jt {
            JuliaType::Struct(name) => {
                // Look up type_id from struct_table
                if let Some(info) = self.shared_ctx.struct_table.get(name) {
                    ValueType::Struct(info.type_id)
                } else {
                    // Handle parametric struct names like "Complex{Float64}" or "Rational{T}"
                    // Extract base name and type arguments
                    let (base_name, type_args) = if let Some(brace_idx) = name.find('{') {
                        let base = &name[..brace_idx];
                        let args_str = &name[brace_idx + 1..name.len() - 1];
                        let args: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
                        (base, args)
                    } else {
                        (name.as_str(), vec![])
                    };

                    // Check if any type argument is a type variable (from where clause)
                    // Type variables should use Any since exact type is unknown at compile time
                    let has_type_variable = !type_args.is_empty()
                        && type_args.iter().any(|arg| {
                            // Type variables are typically single uppercase letters or short names
                            // They won't be in the type system (Int64, Float64, etc.)
                            self.current_type_param_index.contains_key(*arg)
                        });

                    if has_type_variable {
                        // For types with type variables (e.g., Complex{T} where T<:Real),
                        // return Any since exact type is unknown at compile time
                        return ValueType::Any;
                    }

                    // First try exact base name match
                    if let Some(info) = self.shared_ctx.struct_table.get(base_name) {
                        ValueType::Struct(info.type_id)
                    } else {
                        // For names with concrete type parameters (e.g., "Complex{Float64}"),
                        // look for any instantiation of this parametric struct.
                        // Prefer concrete types (Float64, Int64) over Any.
                        let prefix = format!("{}{{", base_name);
                        let mut best_match: Option<(usize, bool)> = None; // (type_id, is_any)

                        for (registered_name, info) in &self.shared_ctx.struct_table {
                            if registered_name.starts_with(&prefix) {
                                let is_any = registered_name.contains("Any");
                                match best_match {
                                    None => best_match = Some((info.type_id, is_any)),
                                    Some((_, true)) if !is_any => {
                                        // Current match is Any, new match is not - prefer new
                                        best_match = Some((info.type_id, is_any));
                                    }
                                    _ => {} // Keep existing match
                                }
                            }
                        }

                        if let Some((type_id, _)) = best_match {
                            return ValueType::Struct(type_id);
                        }

                        // For parametric types with type variables (e.g., "Rational{T}"),
                        // use Any since the exact type is unknown at compile time.
                        // Dispatch will be handled at runtime.
                        ValueType::Any
                    }
                }
            }
            _ => julia_type_to_value_type(jt),
        }
    }

    /// Check if a ValueType represents a struct with the given base name
    pub(super) fn is_struct_type_of(&self, ty: ValueType, base_name: &str) -> bool {
        self.shared_ctx.is_struct_type_of(&ty, base_name)
    }

    /// Get any type_id for a struct with the given base name
    pub(super) fn get_struct_type_id(&self, base_name: &str) -> Option<usize> {
        self.shared_ctx.get_struct_type_id(base_name)
    }

    /// Resolve a struct name, trying both qualified and unqualified versions.
    /// When inside a module (e.g., Dates), prefer the qualified name (e.g., "Dates.Month")
    /// over the unqualified name ("Month") for method dispatch to work correctly.
    pub(super) fn resolve_struct_name(&self, name: &str) -> Option<String> {
        // If inside a module, prefer qualified name first
        if let Some(ref module_path) = self.current_module_path {
            let qualified = format!("{}.{}", module_path, name);
            if self.shared_ctx.struct_table.contains_key(&qualified) {
                return Some(qualified);
            }
        }

        // Try exact name (unqualified or already qualified)
        if self.shared_ctx.struct_table.contains_key(name) {
            // Check if there's a qualified version (module struct imported with short name)
            // For correct method dispatch, we need to use the qualified name (e.g., "Dates.Day")
            // even when called from outside the module via `using Dates`
            for key in self.shared_ctx.struct_table.keys() {
                if key.ends_with(&format!(".{}", name)) && key != name {
                    // Found qualified version - use it for correct method dispatch
                    return Some(key.clone());
                }
            }
            return Some(name.to_string());
        }

        // Not found
        None
    }

    /// Resolve a parametric struct name, returning the qualified version if available.
    /// For imported module structs (e.g., Point after `using .MyGeometry`),
    /// returns the qualified name (e.g., "MyGeometry.Point") for correct method dispatch.
    pub(super) fn resolve_parametric_struct_name(&self, name: &str) -> Option<String> {
        // First check if the exact name exists in parametric_structs
        if self.shared_ctx.parametric_structs.contains_key(name) {
            // Check if there's a qualified version (module struct imported with short name)
            // Look for "Module.name" pattern in parametric_structs
            for key in self.shared_ctx.parametric_structs.keys() {
                if key.ends_with(&format!(".{}", name)) && key != name {
                    // Found qualified version - use it for correct method dispatch
                    return Some(key.clone());
                }
            }
            // No qualified version, use the name as-is
            return Some(name.to_string());
        }

        // Also search for qualified names even when unqualified name doesn't exist
        // This handles the case when we're inside a module and only qualified name is registered
        for key in self.shared_ctx.parametric_structs.keys() {
            if key.ends_with(&format!(".{}", name)) {
                return Some(key.clone());
            }
        }

        None
    }

    pub(super) fn emit(&mut self, i: Instr) {
        self.code.push(i);
    }

    /// Emit a function call instruction, choosing between Call and CallSpecialize.
    /// If the function has a specialization entry (needs_specialization was true),
    /// emit CallSpecialize to enable Lazy AoT compilation.
    pub(super) fn emit_call_or_specialize(
        &mut self,
        _func_name: &str,
        func_index: usize,
        arg_count: usize,
    ) {
        if let Some(&spec_idx) = self.shared_ctx.spec_func_mapping.get(&func_index) {
            self.emit(Instr::CallSpecialize(spec_idx, arg_count));
        } else {
            self.emit(Instr::Call(func_index, arg_count));
        }
    }

    pub(super) fn here(&self) -> usize {
        self.code.len()
    }

    pub(super) fn patch_jump(&mut self, at: usize, target: usize) {
        self.code[at] = match &self.code[at] {
            Instr::Jump(_) => Instr::Jump(target),
            Instr::JumpIfZero(_) => Instr::JumpIfZero(target),
            _ => return,
        };
    }

    /// Patch all @goto jumps with the corresponding @label positions.
    /// This must be called after all statements have been compiled.
    /// Returns an error if any @goto references an undefined label.
    pub(super) fn patch_goto_jumps(&mut self) -> CResult<()> {
        for (patch_pos, label_name) in &self.goto_patches {
            if let Some(&label_pos) = self.label_positions.get(label_name) {
                self.code[*patch_pos] = Instr::Jump(label_pos);
            } else {
                return types::err(format!(
                    "@goto references undefined label: @label {}",
                    label_name
                ));
            }
        }
        // Clear after patching to avoid double-patching issues
        self.goto_patches.clear();
        self.label_positions.clear();
        Ok(())
    }

    pub(super) fn new_temp(&mut self, prefix: &str) -> String {
        self.temp_counter += 1;
        format!("__{}_{}", prefix, self.temp_counter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::ValueType;

    // ── is_integer_type ───────────────────────────────────────────────────────

    #[test]
    fn test_is_integer_type_signed() {
        assert!(is_integer_type(&ValueType::I64), "I64 should be integer");
        assert!(is_integer_type(&ValueType::I8), "I8 should be integer");
        assert!(is_integer_type(&ValueType::I16), "I16 should be integer");
        assert!(is_integer_type(&ValueType::I32), "I32 should be integer");
        assert!(is_integer_type(&ValueType::I128), "I128 should be integer");
    }

    #[test]
    fn test_is_integer_type_unsigned() {
        assert!(is_integer_type(&ValueType::U8), "U8 should be integer");
        assert!(is_integer_type(&ValueType::U16), "U16 should be integer");
        assert!(is_integer_type(&ValueType::U32), "U32 should be integer");
        assert!(is_integer_type(&ValueType::U64), "U64 should be integer");
        assert!(is_integer_type(&ValueType::U128), "U128 should be integer");
    }

    #[test]
    fn test_is_integer_type_non_integer() {
        assert!(!is_integer_type(&ValueType::F64), "F64 is not integer");
        assert!(!is_integer_type(&ValueType::F32), "F32 is not integer");
        assert!(!is_integer_type(&ValueType::Bool), "Bool is not integer");
        assert!(!is_integer_type(&ValueType::Str), "Str is not integer");
        assert!(!is_integer_type(&ValueType::Any), "Any is not integer");
        assert!(!is_integer_type(&ValueType::Nothing), "Nothing is not integer");
    }

    // ── is_float_type ─────────────────────────────────────────────────────────

    #[test]
    fn test_is_float_type_floats() {
        assert!(is_float_type(&ValueType::F64), "F64 should be float");
        assert!(is_float_type(&ValueType::F32), "F32 should be float");
        assert!(is_float_type(&ValueType::F16), "F16 should be float");
    }

    #[test]
    fn test_is_float_type_non_float() {
        assert!(!is_float_type(&ValueType::I64), "I64 is not float");
        assert!(!is_float_type(&ValueType::Bool), "Bool is not float");
        assert!(!is_float_type(&ValueType::Str), "Str is not float");
        assert!(!is_float_type(&ValueType::Any), "Any is not float");
    }

    // ── is_numeric_type ───────────────────────────────────────────────────────

    #[test]
    fn test_is_numeric_type_integers_and_floats() {
        assert!(is_numeric_type(&ValueType::I64), "I64 is numeric");
        assert!(is_numeric_type(&ValueType::F64), "F64 is numeric");
        assert!(is_numeric_type(&ValueType::I8), "I8 is numeric");
        assert!(is_numeric_type(&ValueType::F32), "F32 is numeric");
    }

    #[test]
    fn test_is_numeric_type_non_numeric() {
        assert!(!is_numeric_type(&ValueType::Bool), "Bool is not numeric");
        assert!(!is_numeric_type(&ValueType::Str), "Str is not numeric");
        assert!(!is_numeric_type(&ValueType::Any), "Any is not numeric");
        assert!(!is_numeric_type(&ValueType::Nothing), "Nothing is not numeric");
        assert!(!is_numeric_type(&ValueType::Char), "Char is not numeric");
    }

    // ── is_singleton_type ─────────────────────────────────────────────────────

    #[test]
    fn test_is_singleton_type_singletons() {
        assert!(is_singleton_type(&ValueType::Nothing), "Nothing is singleton");
        assert!(is_singleton_type(&ValueType::DataType), "DataType is singleton");
        assert!(is_singleton_type(&ValueType::Symbol), "Symbol is singleton");
        assert!(is_singleton_type(&ValueType::Char), "Char is singleton");
    }

    #[test]
    fn test_is_singleton_type_non_singletons() {
        assert!(!is_singleton_type(&ValueType::I64), "I64 is not singleton");
        assert!(!is_singleton_type(&ValueType::F64), "F64 is not singleton");
        assert!(!is_singleton_type(&ValueType::Str), "Str is not singleton");
        assert!(!is_singleton_type(&ValueType::Bool), "Bool is not singleton");
        assert!(!is_singleton_type(&ValueType::Any), "Any is not singleton");
    }
}
