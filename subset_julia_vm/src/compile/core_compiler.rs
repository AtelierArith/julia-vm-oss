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

use crate::ir::core::{Block, Function};
use crate::types::{JuliaType, TypeParam};
use crate::vm::{Instr, ResolvedFunctionOperands, ValueType};

use super::context::SharedCompileContext;
use super::method_table::MethodTable;
use super::type_helpers::julia_type_to_value_type;
use super::types::{self, CResult};

pub(super) type ResolvedUsingImport = (String, Option<HashSet<String>>);

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
    /// Locals that have been initialized by code emitted before the current
    /// point. `locals` is pre-populated by whole-block inference, so it can also
    /// contain future sibling-scope names that do not have a runtime slot yet.
    pub(super) initialized_locals: HashSet<String>,
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
    /// Local/static function aliases such as `g = f`.
    ///
    /// This preserves the original generic function name for compile-time
    /// operations that need method-table lookup rather than just a Function
    /// value, e.g. `invoke(g, Tuple{T}, x)` (Issue #4290).
    pub(super) function_aliases: HashMap<String, String>,
    /// Local/static DataType value aliases such as `sig = Tuple{Number}`.
    ///
    /// This preserves type-object values for compile-time surfaces that still
    /// require a statically known type object, while allowing users to spell the
    /// type through a variable as upstream Julia permits (Issue #4290).
    pub(super) type_value_aliases: HashMap<String, JuliaType>,
    pub(super) method_tables: &'a HashMap<String, MethodTable>,
    /// Module name -> Set of function names defined in that module
    pub(super) module_functions: &'a HashMap<String, HashSet<String>>,
    /// Module name -> Set of exported function names (empty = all exported)
    /// Kept to preserve compile-context contract while export handling evolves.
    #[allow(dead_code)]
    pub(super) module_exports: &'a HashMap<String, HashSet<String>>,
    /// Set of function names that are available via `using` (respects export + selective import)
    pub(super) imported_functions: &'a HashSet<String>,
    /// User-defined globals hidden while compiling Base/prelude code.
    pub(super) hidden_user_globals: HashSet<String>,
    /// Set of module names imported via `using` (for backward compatibility)
    pub(super) usings: &'a HashSet<String>,
    /// Resolved `using` statements for this lexical scope. `None` imports the
    /// module's exports; `Some` is a selective `using M: name` set.
    pub(super) resolved_usings: Vec<ResolvedUsingImport>,
    pub(super) shared_ctx: &'a mut SharedCompileContext,
    pub(super) temp_counter: usize,
    /// Stack of active loops for break/continue support
    pub(super) loop_stack: Vec<LoopContext>,
    /// Stack of active finally blocks for return/break/continue handling
    pub(super) finally_stack: Vec<FinallyContext>,
    /// Whether we're in a function body (strict undefined var check) or module/main (lenient)
    pub(super) strict_undefined_check: bool,
    /// Depth of compiler-managed local scopes inside module/main compilation.
    ///
    /// `@testset` bodies are compiled by the module/main compiler, but they are
    /// Julia local scopes. Assignments inside them must be able to shadow
    /// top-level constants such as `pi` instead of being treated as module
    /// constant reassignments (Issue #5991).
    pub(super) local_scope_depth: usize,
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
    /// Names of `where`-clause type parameters that are recoverable from a
    /// constructor argument (i.e. they appear in some parameter's type
    /// annotation, like `Bar(x::T)`), so the runtime can bind them by argument
    /// inference. Explicit-only parameters (`Foo{T}(x)` with an untyped `x`)
    /// are excluded because their value is only known from the call site's
    /// `{...}`, which is not yet plumbed into the constructor frame
    /// (Issue #5059).
    pub(super) ctor_arg_bound_type_vars: HashSet<String>,
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
    /// Names imported into the current module via `using`/`import` (Issue #7575).
    /// An imported name shares one generic function with its source module, so an
    /// unqualified call must keep pooling dispatch candidates across modules and
    /// is excluded from the module-owned-function redirect.
    pub(super) current_module_imports: HashSet<String>,
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
    /// Issue #8118: authoritative capture sets for the directly-nested functions
    /// of the body currently being compiled, keyed by qualified name
    /// (`parent#nested`). Populated by `prescan_mutual_closure_captures` BEFORE
    /// the body's statements compile. A non-empty entry means the nested
    /// function participates in a mutually-recursive closure group that captures
    /// an enclosing local; its `FunctionDef` uses this set verbatim (sibling
    /// function names excluded — those are reconstructed at the call site, not
    /// data-captured) instead of recomputing free variables.
    pub(super) mutual_closure_captures: std::collections::HashMap<String, HashSet<String>>,
    /// Current enclosing function name (for creating qualified nested function names).
    /// Used to disambiguate nested functions with the same name in different parent functions.
    pub(super) current_function_name: Option<String>,
    /// True while compiling Base/prelude function bodies. Unqualified calls inside
    /// Base must keep resolving to Base/builtin behavior even when a loaded module
    /// defines the same public name (e.g. MacroTools.match).
    pub(super) in_base_function_scope: bool,
    /// Top-level/module `const` bindings whose first assignment has completed.
    pub(super) const_bindings: HashSet<String>,
    /// `const` bindings declared immediately before their first assignment.
    pub(super) pending_const_bindings: HashSet<String>,
    /// Compile-time values for `const` bindings that fold to `ConstValue`.
    pub(super) const_values: HashMap<String, crate::compile::lattice::types::ConstValue>,
    /// True while compiling the operand of an `@inbounds` macro.
    pub(super) inbounds_context: bool,
    /// `(array_var, index_var)` pairs proven in-bounds by the current loop.
    pub(super) proven_inbounds_indices: Vec<(String, String)>,
    /// Names declared `global` in the current function scope (`global x`).
    ///
    /// Reads of these names resolve to the module-level (frame 0) binding and
    /// writes route there via `StoreGlobalAny`, instead of introducing a local
    /// binding in the function frame (Issues #5548, #5549). Populated by a
    /// pre-scan of the function body before statement compilation begins.
    pub(super) declared_globals: HashSet<String>,
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

pub(super) fn static_assignment_types_compatible(target: &ValueType, incoming: &ValueType) -> bool {
    target == incoming
        || (is_numeric_type(target) && is_numeric_type(incoming))
        || matches!(
            (target, incoming),
            (ValueType::Array, ValueType::ArrayOf(_, _))
        )
        || matches!(
            (target, incoming),
            (ValueType::ArrayOf(_, _), ValueType::Array)
        )
}

fn imported_submodule_aliases(
    module_functions: &HashMap<String, HashSet<String>>,
    module_exports: &HashMap<String, HashSet<String>>,
    imported_symbols: &HashSet<String>,
    usings: &HashSet<String>,
) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    for using_module in usings {
        let prefix = format!("{}.", using_module);
        let exports = module_exports.get(using_module);
        let all_exported = exports.is_none_or(|e| e.is_empty());
        for module_path in module_functions.keys() {
            let Some(name) = module_path.strip_prefix(&prefix) else {
                continue;
            };
            if name.contains('.') {
                continue;
            }
            if imported_symbols.contains(name)
                && (all_exported || exports.is_some_and(|e| e.contains(name)))
            {
                aliases.insert(name.to_string(), module_path.clone());
            }
        }
    }
    aliases
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
    pub(super) fn infer_shared_function_return_type_with_arg_types(
        &self,
        func: &Function,
        arg_value_types: &[ValueType],
    ) -> ValueType {
        if super::constants::is_generated_function(func) {
            return ValueType::Any;
        }

        // Issue #5425 / #5466: call-site refinement only sees positional
        // arguments. Returning an unannotated optional kwarg must therefore stay
        // dynamic so keyword calls can pass values with any runtime type.
        if crate::compile::returns_unannotated_optional_kwparam_value(func) {
            return ValueType::Any;
        }

        let mut engine = crate::compile::inference::build_shared_inference_engine(
            &self.shared_ctx.struct_table,
            &self.shared_ctx.global_types,
            std::iter::once(func),
        );
        // This single-function engine otherwise can't see other functions'
        // method tables, so a body call like `first(xs)` falls back to the
        // element-type tfunc — wrong when the user has overridden `first`/`last`/
        // `getindex` to return a non-element type, which then mis-coerced the
        // wrapper's result (Issue #6657). Seed only the tables that contain a
        // *user* override (cheap `Arc` clones) so the body re-inference resolves
        // those overrides to the method's declared return type; tables with only
        // Base methods are left out so the tfunc fast path (element-type
        // precision) stays intact for the common, non-overridden case.
        let user_overridden_tables = self.method_tables.iter().filter(|(_, table)| {
            table.methods.iter().any(|method| {
                self.shared_ctx
                    .function_ir_by_global_index
                    .contains_key(&method.global_index)
            })
        });
        engine.seed_initial_method_tables(user_overridden_tables);
        let arg_lattice_types: Vec<_> = arg_value_types
            .iter()
            .map(|vt| {
                crate::compile::bridge::value_type_to_lattice_with_struct_table(
                    vt,
                    &self.shared_ctx.struct_table,
                )
            })
            .collect();
        let return_type = engine.infer_function_with_arg_types(func, &arg_lattice_types);
        crate::compile::bridge::lattice_to_value_type(&return_type)
    }

    pub(super) fn should_accept_body_reinferred_call_return_type(
        &self,
        inferred: &ValueType,
    ) -> bool {
        // Issue #8414: body re-inference is an opportunistic refinement for
        // methods whose table metadata is `Any`. Do not let it narrow such a
        // call to the singleton `Nothing`: assignment code treats `Nothing` as
        // a no-slot value and drops the real runtime result. A method that
        // truly always returns `nothing` should already have `Nothing` metadata.
        !matches!(inferred, ValueType::Any | ValueType::Nothing)
    }

    pub(super) fn function_index_is_generated(&self, global_index: usize) -> bool {
        self.shared_ctx
            .function_ir_by_global_index
            .get(&global_index)
            .is_some_and(super::constants::is_generated_function)
    }

    fn module_has_binding(&self, module: &str, name: &str) -> bool {
        let qualified = format!("{}.{}", module, name);
        self.module_functions
            .get(module)
            .is_some_and(|bindings| bindings.contains(name))
            || self
                .module_constants
                .get(module)
                .is_some_and(|constants| constants.contains(name))
            || self.shared_ctx.type_aliases.contains_key(&qualified)
            || self.type_object_binding_exists(&qualified)
    }

    pub(super) fn type_object_binding_exists(&self, name: &str) -> bool {
        self.abstract_type_names.contains(name)
            || self.shared_ctx.enum_types.contains_key(name)
            || self.shared_ctx.is_primitive_type_name(name)
            || self.shared_ctx.struct_table.contains_key(name)
            || self.shared_ctx.parametric_structs.contains_key(name)
    }

    pub(super) fn resolve_visible_type_object_name(&self, name: &str) -> Option<String> {
        if let Some(module_path) = &self.current_module_path {
            let qualified = format!("{}.{}", module_path, name);
            if self.type_object_binding_exists(&qualified) {
                return Some(qualified);
            }
        }

        for using_module in self.visible_using_modules_for_name(name) {
            let qualified = format!("{}.{}", using_module, name);
            if self.type_object_binding_exists(&qualified) {
                return Some(qualified);
            }
        }

        self.type_object_binding_exists(name)
            .then(|| name.to_string())
    }

    pub(super) fn canonical_module_path(&self, module: &str) -> String {
        if let Some(base_submodule) = module.strip_prefix("Base.") {
            if self.module_functions.contains_key(base_submodule)
                && !super::constants::is_stdlib_module(base_submodule)
            {
                return base_submodule.to_string();
            }
        }
        module.to_string()
    }

    pub(super) fn nested_module_path_in_current_module(&self, name: &str) -> Option<String> {
        let current = self.current_module_path.as_ref()?;
        let qualified = format!("{}.{}", current, name);
        (self.module_functions.contains_key(&qualified)
            || self.module_exports.contains_key(&qualified))
        .then_some(qualified)
    }

    pub(super) fn visible_using_modules_for_name(&self, name: &str) -> Vec<String> {
        let mut modules = Vec::new();
        let mut seen = HashSet::new();

        if self.resolved_usings.is_empty() {
            for using_module in self.usings {
                if self
                    .module_exports
                    .get(using_module)
                    .is_some_and(|exports| exports.contains(name))
                    && seen.insert(using_module.clone())
                {
                    modules.push(using_module.clone());
                }
            }
            return modules;
        }

        for (using_module, selected_symbols) in &self.resolved_usings {
            let visible = match selected_symbols {
                Some(symbols) => {
                    symbols.contains(name) && self.module_has_binding(using_module, name)
                }
                None => self
                    .module_exports
                    .get(using_module)
                    .is_some_and(|exports| {
                        if exports.is_empty() {
                            self.module_has_binding(using_module, name)
                        } else {
                            exports.contains(name)
                        }
                    }),
            };
            if visible && seen.insert(using_module.clone()) {
                modules.push(using_module.clone());
            }
        }
        modules
    }

    pub(super) fn new(
        method_tables: &'a HashMap<String, MethodTable>,
        module_functions: &'a HashMap<String, HashSet<String>>,
        module_exports: &'a HashMap<String, HashSet<String>>,
        imported_functions: &'a HashSet<String>,
        usings: &'a HashSet<String>,
        resolved_usings: Vec<ResolvedUsingImport>,
        shared_ctx: &'a mut SharedCompileContext,
        abstract_type_names: &'a HashSet<String>,
        module_constants: &'a HashMap<String, HashSet<String>>,
    ) -> Self {
        let module_aliases = imported_submodule_aliases(
            module_functions,
            module_exports,
            imported_functions,
            usings,
        );
        Self {
            code: Vec::with_capacity(64),
            locals: HashMap::new(),
            initialized_locals: HashSet::new(),
            julia_type_locals: HashMap::new(),
            function_aliases: HashMap::new(),
            type_value_aliases: HashMap::new(),
            method_tables,
            module_functions,
            module_exports,
            imported_functions,
            hidden_user_globals: HashSet::new(),
            usings,
            resolved_usings,
            shared_ctx,
            temp_counter: 0,
            loop_stack: Vec::new(),
            finally_stack: Vec::new(),
            strict_undefined_check: false, // Default to lenient for module/main
            local_scope_depth: 0,
            current_struct_type_id: None,
            current_parametric_struct_name: None,
            any_params: HashSet::new(), // No params in module/main context
            abstract_numeric_params: HashSet::new(),
            module_aliases,
            abstract_type_names,
            current_type_params: Vec::new(),
            current_type_param_index: HashMap::new(),
            ctor_arg_bound_type_vars: HashSet::new(),
            mixed_type_vars: HashSet::new(), // No mixed type vars in module/main context
            val_type_params: HashSet::new(),
            val_bool_params: HashSet::new(),
            val_symbol_params: HashSet::new(),
            current_module_path: None,
            current_module_imports: HashSet::new(),
            module_constants,
            label_positions: HashMap::new(),
            goto_patches: Vec::new(),
            captured_vars: HashSet::new(),
            mutual_closure_captures: std::collections::HashMap::new(),
            current_function_name: None,
            in_base_function_scope: false,
            const_bindings: HashSet::new(),
            pending_const_bindings: HashSet::new(),
            const_values: HashMap::new(),
            inbounds_context: false,
            proven_inbounds_indices: Vec::new(),
            declared_globals: HashSet::new(),
        }
    }

    pub(super) fn new_for_function(
        method_tables: &'a HashMap<String, MethodTable>,
        module_functions: &'a HashMap<String, HashSet<String>>,
        module_exports: &'a HashMap<String, HashSet<String>>,
        imported_functions: &'a HashSet<String>,
        usings: &'a HashSet<String>,
        resolved_usings: Vec<ResolvedUsingImport>,
        shared_ctx: &'a mut SharedCompileContext,
        abstract_type_names: &'a HashSet<String>,
        module_constants: &'a HashMap<String, HashSet<String>>,
    ) -> Self {
        let module_aliases = imported_submodule_aliases(
            module_functions,
            module_exports,
            imported_functions,
            usings,
        );
        Self {
            code: Vec::with_capacity(64),
            locals: HashMap::new(),
            initialized_locals: HashSet::new(),
            julia_type_locals: HashMap::new(),
            function_aliases: HashMap::new(),
            type_value_aliases: HashMap::new(),
            method_tables,
            module_functions,
            module_exports,
            imported_functions,
            hidden_user_globals: HashSet::new(),
            usings,
            resolved_usings,
            shared_ctx,
            temp_counter: 0,
            loop_stack: Vec::new(),
            finally_stack: Vec::new(),
            strict_undefined_check: true, // Strict for function bodies
            local_scope_depth: 0,
            any_params: HashSet::new(), // Will be populated after creation
            abstract_numeric_params: HashSet::new(), // Will be populated after creation
            module_aliases,
            abstract_type_names,
            current_struct_type_id: None,
            current_parametric_struct_name: None,
            current_type_params: Vec::new(), // Will be set after creation
            current_type_param_index: HashMap::new(), // Will be set after creation
            ctor_arg_bound_type_vars: HashSet::new(), // Will be set after creation
            mixed_type_vars: HashSet::new(), // Will be populated from type inference
            val_type_params: HashSet::new(), // Will be populated from parameter analysis
            val_bool_params: HashSet::new(), // Will be populated from parameter analysis
            val_symbol_params: HashSet::new(), // Will be populated from parameter analysis
            current_module_path: None,       // Will be set after creation
            current_module_imports: HashSet::new(), // Will be set after creation
            module_constants,
            label_positions: HashMap::new(),
            goto_patches: Vec::new(),
            captured_vars: HashSet::new(), // Will be populated for closures
            mutual_closure_captures: std::collections::HashMap::new(),
            current_function_name: None, // Will be set when compiling functions
            in_base_function_scope: false, // Will be set when compiling functions
            const_bindings: HashSet::new(),
            pending_const_bindings: HashSet::new(),
            const_values: HashMap::new(),
            inbounds_context: false,
            proven_inbounds_indices: Vec::new(),
            declared_globals: HashSet::new(),
        }
    }

    pub(super) fn resolve_function_alias_value(
        &self,
        expr: &crate::ir::core::Expr,
    ) -> Option<String> {
        match expr {
            crate::ir::core::Expr::Var(name, _) => {
                if (self.method_tables.contains_key(name)
                    && !self.hidden_user_globals.contains(name))
                    || super::is_base_function(name)
                {
                    Some(name.clone())
                } else {
                    self.function_aliases.get(name).cloned()
                }
            }
            crate::ir::core::Expr::FunctionRef { name, .. } => Some(name.clone()),
            _ => None,
        }
    }

    pub(super) fn push_proven_inbounds_index(&mut self, array_var: &str, index_var: &str) {
        self.proven_inbounds_indices
            .push((array_var.to_string(), index_var.to_string()));
    }

    pub(super) fn pop_proven_inbounds_index(&mut self) {
        self.proven_inbounds_indices.pop();
    }

    pub(super) fn is_proven_inbounds_index(
        &self,
        array: &crate::ir::core::Expr,
        index: &crate::ir::core::Expr,
    ) -> bool {
        let crate::ir::core::Expr::Var(array_var, _) = array else {
            return false;
        };
        let crate::ir::core::Expr::Var(index_var, _) = index else {
            return false;
        };
        self.proven_inbounds_indices
            .iter()
            .rev()
            .any(|(arr, idx)| arr == array_var && idx == index_var)
    }

    pub(super) fn resolve_static_datatype_value(
        &self,
        expr: &crate::ir::core::Expr,
    ) -> Option<JuliaType> {
        match expr {
            crate::ir::core::Expr::Builtin {
                name: crate::ir::core::BuiltinOp::TypeOf,
                args,
                ..
            } => match args.first() {
                Some(crate::ir::core::Expr::Literal(
                    crate::ir::core::Literal::Str(type_name),
                    _,
                )) => Some(
                    JuliaType::from_name(type_name)
                        .unwrap_or_else(|| JuliaType::from_name_or_struct(type_name)),
                ),
                _ => None,
            },
            crate::ir::core::Expr::Var(name, _) => self.type_value_aliases.get(name).cloned(),
            _ => None,
        }
    }

    /// Convert JuliaType to ValueType, resolving struct types using the struct table.
    pub(super) fn julia_type_to_value_type_with_ctx(&self, jt: &JuliaType) -> ValueType {
        match jt {
            JuliaType::Bottom => ValueType::Union(Vec::new()),
            JuliaType::Union(types) => ValueType::Union(
                types
                    .iter()
                    .map(|ty| self.julia_type_to_value_type_with_ctx(ty))
                    .collect::<Vec<_>>(),
            ),
            JuliaType::Struct(name) => {
                // RNG type annotations (Xoshiro/StableRNG/MersenneTwister/
                // TaskLocalRNG/AbstractRNG) map to ValueType::Rng so randn(rng) /
                // rand(rng) on such a param compile to the scalar-from-rng form
                // (Issue #7231).
                if super::type_helpers::is_rng_type_name(name) {
                    return ValueType::Rng;
                }
                if let Some(memory_type) =
                    super::type_helpers::memory_struct_name_to_value_type(name)
                {
                    return memory_type;
                }
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

    pub(super) fn resolve_visible_type_alias(&self, name: &str) -> Option<String> {
        if let Some(module_path) = &self.current_module_path {
            let qualified = format!("{}.{}", module_path, name);
            if let Some(target) = self.shared_ctx.type_aliases.get(&qualified) {
                return Some(target.clone());
            }
        }

        for using_module in self.visible_using_modules_for_name(name) {
            let qualified = format!("{}.{}", using_module, name);
            if let Some(target) = self.shared_ctx.type_aliases.get(&qualified) {
                return Some(target.clone());
            }
        }

        self.shared_ctx.type_aliases.get(name).cloned()
    }

    /// Resolve a parametric struct name, returning the qualified version if available.
    /// For imported module structs (e.g., Point after `using .MyGeometry`),
    /// returns the qualified name (e.g., "MyGeometry.Point") for correct method dispatch.
    pub(super) fn resolve_parametric_struct_name(&self, name: &str) -> Option<String> {
        if let Some(unqualified) = name.strip_prefix("Base.") {
            if self.shared_ctx.parametric_structs.contains_key(unqualified) {
                return Some(unqualified.to_string());
            }
        }

        // Scope-aware resolution (Issue #8313): a bare name must resolve to a
        // struct that is actually in scope — defined in the current module, or
        // brought in by `using` — rather than an arbitrary same-named struct
        // elsewhere. Without this, a user `Perm` (from `using .M`) collides with
        // the bundled `Base.Order.Perm` the program never imported, and the
        // suffix-match fallbacks below (which iterate a `HashMap`) pick one
        // nondeterministically — `Perm([1,2,3])` then sometimes resolves to the
        // 2-parameter `Order.Perm`. Mirrors `resolve_visible_type_alias`.
        if let Some(module_path) = &self.current_module_path {
            let qualified = format!("{}.{}", module_path, name);
            if self.shared_ctx.parametric_structs.contains_key(&qualified) {
                return Some(qualified);
            }
        }
        for using_module in self.visible_using_modules_for_name(name) {
            let qualified = format!("{}.{}", using_module, name);
            if self.shared_ctx.parametric_structs.contains_key(&qualified) {
                return Some(qualified);
            }
        }

        // First check if the exact name exists in parametric_structs
        if self.shared_ctx.parametric_structs.contains_key(name) {
            // Check if there's a qualified version (module struct used under its
            // short name) and use it for correct method dispatch. Choose
            // deterministically — `HashMap` iteration order is unstable, so with a
            // same-named struct in another (out-of-scope) module the previous
            // "first match" could vary between runs (Issue #8313).
            if let Some(key) = self.first_qualified_struct_key(name) {
                return Some(key);
            }
            // No qualified version, use the name as-is
            return Some(name.to_string());
        }

        // Also search for qualified names even when the unqualified name doesn't
        // exist (e.g. when only the qualified name is registered). Deterministic.
        self.first_qualified_struct_key(name)
    }

    /// Smallest (deterministic) `Module.name` key among registered parametric
    /// structs whose qualified name ends in `.name`. Used as the out-of-scope
    /// fallback for `resolve_parametric_struct_name`; the scope-aware lookups run
    /// first, so this only disambiguates names with no in-scope binding, where any
    /// stable choice beats `HashMap`-order nondeterminism (Issue #8313).
    fn first_qualified_struct_key(&self, name: &str) -> Option<String> {
        let suffix = format!(".{}", name);
        self.shared_ctx
            .parametric_structs
            .keys()
            .filter(|key| key.ends_with(&suffix) && key.as_str() != name)
            .min()
            .cloned()
    }

    pub(super) fn emit(&mut self, i: Instr) {
        self.code.push(i);
    }

    pub(super) fn emit_function_value(&mut self, name: &str) {
        let candidate_indices = self
            .method_tables
            .get(name)
            .map(|table| {
                table
                    .methods
                    .iter()
                    .map(|method| method.global_index)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if candidate_indices.is_empty() {
            self.emit(Instr::PushFunction(name.to_string()));
        } else {
            self.emit(Instr::PushResolvedFunction(Box::new(
                ResolvedFunctionOperands {
                    name: name.to_string(),
                    candidate_indices,
                },
            )));
        }
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
            if self.inbounds_context {
                self.emit(Instr::CallSpecializeInbounds(spec_idx, arg_count));
            } else {
                self.emit(Instr::CallSpecialize(spec_idx, arg_count));
            }
        } else if self.inbounds_context {
            self.emit(Instr::CallInbounds(func_index, arg_count));
        } else {
            self.emit(Instr::CallResolved(func_index, arg_count));
        }
    }

    pub(super) fn here(&self) -> usize {
        self.code.len()
    }

    pub(super) fn patch_jump(&mut self, at: usize, target: usize) {
        self.code[at] = match &self.code[at] {
            Instr::Jump(_) => Instr::Jump(target),
            Instr::JumpIfZero(_) => Instr::JumpIfZero(target),
            // Directional comparison jumps are emitted as exit tests by the
            // constant-step `for` loop fast path (Issue #5166).
            Instr::JumpIfLtI64(_) => Instr::JumpIfLtI64(target),
            Instr::JumpIfGtI64(_) => Instr::JumpIfGtI64(target),
            Instr::JumpIfGtI64Slots(lhs_slot, rhs_slot, _) => {
                Instr::JumpIfGtI64Slots(*lhs_slot, *rhs_slot, target)
            }
            Instr::AddConstI64SlotAndJumpIfLe(slot, delta, stop_slot, _) => {
                Instr::AddConstI64SlotAndJumpIfLe(*slot, *delta, *stop_slot, target)
            }
            Instr::JumpIfLeI64(_) => Instr::JumpIfLeI64(target),
            Instr::JumpIfGeI64(_) => Instr::JumpIfGeI64(target),
            Instr::JumpIfEqI64(_) => Instr::JumpIfEqI64(target),
            Instr::JumpIfNeI64(_) => Instr::JumpIfNeI64(target),
            Instr::JumpIfEqF64(_) => Instr::JumpIfEqF64(target),
            Instr::JumpIfNeF64(_) => Instr::JumpIfNeF64(target),
            Instr::JumpIfNotLtF64(_) => Instr::JumpIfNotLtF64(target),
            Instr::JumpIfNotGtF64(_) => Instr::JumpIfNotGtF64(target),
            Instr::JumpIfNotLeF64(_) => Instr::JumpIfNotLeF64(target),
            Instr::JumpIfNotGeF64(_) => Instr::JumpIfNotGeF64(target),
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
        assert!(
            !is_integer_type(&ValueType::Nothing),
            "Nothing is not integer"
        );
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
        assert!(
            !is_numeric_type(&ValueType::Nothing),
            "Nothing is not numeric"
        );
        assert!(!is_numeric_type(&ValueType::Char), "Char is not numeric");
    }

    // ── is_singleton_type ─────────────────────────────────────────────────────

    #[test]
    fn test_is_singleton_type_singletons() {
        assert!(
            is_singleton_type(&ValueType::Nothing),
            "Nothing is singleton"
        );
        assert!(
            is_singleton_type(&ValueType::DataType),
            "DataType is singleton"
        );
        assert!(is_singleton_type(&ValueType::Symbol), "Symbol is singleton");
        assert!(is_singleton_type(&ValueType::Char), "Char is singleton");
    }

    #[test]
    fn test_is_singleton_type_non_singletons() {
        assert!(!is_singleton_type(&ValueType::I64), "I64 is not singleton");
        assert!(!is_singleton_type(&ValueType::F64), "F64 is not singleton");
        assert!(!is_singleton_type(&ValueType::Str), "Str is not singleton");
        assert!(
            !is_singleton_type(&ValueType::Bool),
            "Bool is not singleton"
        );
        assert!(!is_singleton_type(&ValueType::Any), "Any is not singleton");
    }
}
