pub mod abstract_;
mod closure_box;
pub mod expr;
pub mod function;
pub(crate) mod generated_unquote;
mod macro_runtime;
mod macros_registry;
pub mod primitive;
pub mod stmt;
pub mod struct_;
pub mod type_alias;

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use std::collections::HashMap;

use crate::error::{IncludeError, UnsupportedFeature, UnsupportedFeatureKind};
use crate::include::{read_include_file, resolve_include_path};
use crate::ir::core::{
    AbstractTypeDef, Block, BuiltinOp, Expr, Function, Literal, MacroDef, Module, PrimitiveTypeDef,
    Program, Stmt, StructDef, TypeAliasDef, UsingImport,
};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::parser::{ParseOutcome, Parser};
use crate::span::Span;
use crate::stdlib_loader::{ensure_bundled_package_macros_loaded, ensure_stdlib_macros_loaded};
use crate::types::TypeParam;
use macros_registry::check_type_compatibility;

pub use macros_registry::{get_node_macro_type, MacroHygieneInfo, MacroParamType, StoredMacroDef};

pub type LowerResult<T> = Result<T, UnsupportedFeature>;

/// Result type for include operations that can fail with IncludeError.
pub type IncludeResult<T> = Result<T, IncludeError>;

/// Returns `true` when the node at `idx` is a top-level docstring — a
/// `StringLiteral` whose next non-comment sibling is a definition. Matches
/// Julia's docstring convention (`"""doc""" function f end` → `@doc "doc" f`)
/// so the string is documentation rather than a value-producing statement.
fn is_top_level_docstring(walker: &CstWalker<'_>, children: &[Node<'_>], idx: usize) -> bool {
    let mut next_idx = idx + 1;
    while let Some(next) = children.get(next_idx) {
        match walker.kind(next) {
            NodeKind::LineComment | NodeKind::BlockComment => {
                next_idx += 1;
            }
            next_kind => return is_docstring_target_kind(next_kind),
        }
    }
    false
}

pub(crate) fn contains_macro_call(walker: &CstWalker<'_>, node: Node<'_>) -> bool {
    walker.kind(&node) == NodeKind::MacroCall
        || walker
            .named_children(&node)
            .into_iter()
            .any(|child| contains_macro_call(walker, child))
}

fn lower_function_all_with_macro_ctx_if_needed<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Vec<Function>> {
    if contains_macro_call(walker, node) {
        function::lower_function_all_with_ctx(walker, node, lambda_ctx)
    } else {
        function::lower_function_all(walker, node)
    }
}

fn lower_operator_method_with_macro_ctx_if_needed<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Function> {
    if contains_macro_call(walker, node) {
        function::lower_operator_method_with_ctx(walker, node, lambda_ctx)
    } else {
        function::lower_operator_method(walker, node)
    }
}

fn lower_short_function_all_with_macro_ctx_if_needed<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Vec<Function>> {
    if contains_macro_call(walker, node) {
        function::lower_short_function_all_with_ctx(walker, node, lambda_ctx)
    } else {
        function::lower_short_function_all(walker, node)
    }
}

fn lower_stmt_with_macro_ctx_if_needed<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    macro_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match macro_ctx.filter(|_| contains_macro_call(walker, node)) {
        Some(ctx) => stmt::lower_stmt_with_ctx(walker, node, ctx),
        None => stmt::lower_stmt(walker, node),
    }
}

/// Kinds of top-level constructs that absorb a preceding string literal as a
/// docstring. Mirrors the definition-producing arms of `lower_source_file`.
fn is_docstring_target_kind(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::FunctionDefinition
            | NodeKind::ShortFunctionDefinition
            | NodeKind::MacroDefinition
            | NodeKind::StructDefinition
            | NodeKind::MutableStructDefinition
            | NodeKind::AbstractDefinition
            | NodeKind::PrimitiveDefinition
            | NodeKind::ModuleDefinition
            | NodeKind::BaremoduleDefinition
            | NodeKind::ConstStatement
            | NodeKind::Assignment
            | NodeKind::MacroCall
    )
}

/// Pre-scan a CST subtree and register every user-defined type alias (both the
/// non-parametric `const Name = T` form and the parametric `Name{P...} = T`
/// form) into the thread-local alias table (Issue #5055). Running this before
/// statement lowering lets alias uses anywhere in the program resolve to their
/// target type strings, independent of source order. Descends into module
/// bodies so module-level aliases are visible too.
fn prescan_and_register_type_aliases(walker: &CstWalker<'_>, node: Node<'_>) {
    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::ConstStatement => {
                if let Some(alias) = stmt::try_extract_type_alias(walker, child) {
                    type_alias::register(&alias.name, alias.params.clone(), &alias.target_type);
                }
            }
            NodeKind::Assignment => {
                if let Some(alias) = stmt::try_extract_type_alias_from_assignment(walker, child) {
                    type_alias::register(&alias.name, alias.params.clone(), &alias.target_type);
                }
            }
            NodeKind::ModuleDefinition | NodeKind::BaremoduleDefinition => {
                // Recurse into module bodies (their `Block` child) so aliases
                // defined inside modules are registered as well.
                for inner in walker.named_children(&child) {
                    if walker.kind(&inner) == NodeKind::Block {
                        prescan_and_register_type_aliases(walker, inner);
                    }
                }
            }
            NodeKind::Block => {
                prescan_and_register_type_aliases(walker, child);
            }
            _ => {}
        }
    }
}

fn extract_top_level_function_defs(stmt: Stmt) -> Result<Vec<Function>, Box<Stmt>> {
    match stmt {
        Stmt::FunctionDef { func, .. } => Ok(vec![*func]),
        Stmt::Block(block)
            if block
                .stmts
                .iter()
                .all(|stmt| matches!(stmt, Stmt::FunctionDef { .. })) =>
        {
            let mut funcs = Vec::new();
            for stmt in block.stmts {
                if let Stmt::FunctionDef { func, .. } = stmt {
                    funcs.push(*func);
                }
            }
            Ok(funcs)
        }
        other => Err(Box::new(other)),
    }
}

fn drain_macro_expanded_structs(ctx: &LambdaContext, structs: &mut Vec<StructDef>) {
    structs.extend(ctx.take_macro_expanded_structs());
}

fn reject_macro_expanded_structs_in_non_toplevel(
    ctx: &LambdaContext,
    span: crate::span::Span,
) -> LowerResult<()> {
    let structs = ctx.take_macro_expanded_structs();
    if structs.is_empty() {
        return Ok(());
    }
    Err(
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
            "macro-expanded struct definitions are only supported at top level or module body",
        ),
    )
}

/// Context for collecting lifted lambda functions during lowering.
/// Used when lambdas appear as arguments to function calls (e.g., `map(x -> x^2, arr)`).
/// Also tracks which modules have been imported via `using` for macro availability.
/// Also stores user-defined macro definitions for expansion.
pub struct LambdaContext {
    lifted_functions: RefCell<Vec<Function>>,
    lambda_counter: RefCell<usize>,
    /// Modules imported via `using` statements, used to gate macro availability
    usings: RefCell<HashSet<String>>,
    /// User-defined macro definitions, indexed by name, supporting multiple arities
    macros: RefCell<HashMap<String, Vec<StoredMacroDef>>>,
    /// Top-level functions visible to expansion-time macro execution.
    compile_time_functions: RefCell<Vec<Function>>,
    /// Top-level type definitions visible to expansion-time macro execution. A
    /// bundled-package macro (e.g. Plots' `@animate`) is expanded by compiling and
    /// running ALL `compile_time_functions`; if one of those references a user struct
    /// (e.g. `step!(l::Lorenz)` reading `l.x`), the expansion program needs the type
    /// definitions too, or compilation fails with "Unknown field: …".
    compile_time_structs: RefCell<Vec<StructDef>>,
    /// Struct definitions produced by macro expansion while lowering a
    /// top-level/module-body statement. The surrounding top-level lowering pass
    /// drains this immediately and adds the definitions to Program/Module metadata
    /// (Issue #7915).
    macro_expanded_structs: RefCell<Vec<StructDef>>,
    compile_time_abstract_types: RefCell<Vec<AbstractTypeDef>>,
    compile_time_primitive_types: RefCell<Vec<PrimitiveTypeDef>>,
    /// Hygiene info for module-defined macros (Issue #7355 / #7350 A4): macro name
    /// → defining module metadata. Drives qualification of a macro's non-`esc`
    /// call targets so unexported module members resolve in the macro's
    /// *defining* module rather than the caller scope.
    module_macro_hygiene: RefCell<HashMap<String, MacroHygieneInfo>>,
    /// Active hygiene frames while converting a module-defined macro's returned AST
    /// to IR. Each frame carries the defining module, its member names, and the
    /// current `esc` nesting depth (qualification is suppressed inside `esc`).
    macro_hygiene_stack: RefCell<Vec<MacroHygieneFrame>>,
    /// Current file path (for @__FILE__ and @__DIR__ macros)
    /// None means REPL or unknown source
    current_file: RefCell<Option<String>>,
    /// Function `where` type parameters currently in scope while lowering a
    /// function body. Runtime macro-return conversion uses this to avoid
    /// stringifying caller type variables into static type literals.
    active_type_params: RefCell<Vec<HashSet<String>>>,
    /// Stack of enclosing module names while lowering module bodies. A macro
    /// expanded inside `module M ... end` must receive `M` as `__module__`, not
    /// the hard-coded `Main` (Issue #7919). The top of the stack is the
    /// innermost active module; an empty stack means top-level (`Main`).
    current_module_stack: RefCell<Vec<String>>,
    prefer_nested_lambdas: Cell<bool>,
}

/// One active hygiene context for a module-defined macro being expanded.
struct MacroHygieneFrame {
    module: String,
    members: HashSet<String>,
    esc_depth: usize,
}

impl LambdaContext {
    pub fn new() -> Self {
        Self {
            lifted_functions: RefCell::new(Vec::new()),
            lambda_counter: RefCell::new(0),
            usings: RefCell::new(HashSet::new()),
            macros: RefCell::new(HashMap::new()),
            compile_time_functions: RefCell::new(Vec::new()),
            compile_time_structs: RefCell::new(Vec::new()),
            macro_expanded_structs: RefCell::new(Vec::new()),
            compile_time_abstract_types: RefCell::new(Vec::new()),
            compile_time_primitive_types: RefCell::new(Vec::new()),
            module_macro_hygiene: RefCell::new(HashMap::new()),
            macro_hygiene_stack: RefCell::new(Vec::new()),
            current_file: RefCell::new(None),
            active_type_params: RefCell::new(Vec::new()),
            current_module_stack: RefCell::new(Vec::new()),
            prefer_nested_lambdas: Cell::new(false),
        }
    }

    /// Create a new LambdaContext with a specific file path.
    /// Used when lowering files (not REPL).
    pub fn with_file(file_path: Option<String>) -> Self {
        Self {
            lifted_functions: RefCell::new(Vec::new()),
            lambda_counter: RefCell::new(0),
            usings: RefCell::new(HashSet::new()),
            macros: RefCell::new(HashMap::new()),
            compile_time_functions: RefCell::new(Vec::new()),
            compile_time_structs: RefCell::new(Vec::new()),
            macro_expanded_structs: RefCell::new(Vec::new()),
            compile_time_abstract_types: RefCell::new(Vec::new()),
            compile_time_primitive_types: RefCell::new(Vec::new()),
            module_macro_hygiene: RefCell::new(HashMap::new()),
            macro_hygiene_stack: RefCell::new(Vec::new()),
            current_file: RefCell::new(file_path),
            active_type_params: RefCell::new(Vec::new()),
            current_module_stack: RefCell::new(Vec::new()),
            prefer_nested_lambdas: Cell::new(false),
        }
    }

    pub fn prefer_nested_lambdas(&self) -> bool {
        self.prefer_nested_lambdas.get()
    }

    pub fn with_prefer_nested_lambdas<T>(&self, value: bool, f: impl FnOnce() -> T) -> T {
        let previous = self.prefer_nested_lambdas.replace(value);
        let result = f();
        self.prefer_nested_lambdas.set(previous);
        result
    }

    pub fn with_current_file<T>(&self, file_path: Option<String>, f: impl FnOnce() -> T) -> T {
        let previous = self.current_file.replace(file_path);
        let result = f();
        self.current_file.replace(previous);
        result
    }

    pub fn with_active_type_params<T>(
        &self,
        type_params: &[TypeParam],
        f: impl FnOnce() -> T,
    ) -> T {
        if type_params.is_empty() {
            return f();
        }
        self.active_type_params
            .borrow_mut()
            .push(type_params.iter().map(|tp| tp.name.clone()).collect());
        let result = f();
        self.active_type_params.borrow_mut().pop();
        result
    }

    pub fn is_active_type_param(&self, name: &str) -> bool {
        self.active_type_params
            .borrow()
            .iter()
            .rev()
            .any(|frame| frame.contains(name))
    }

    /// Push the name of the module whose body is currently being lowered, so a
    /// macro expanded inside it receives that module as `__module__` instead of
    /// the hard-coded `Main` (Issue #7919). Must be paired with
    /// [`Self::pop_current_module`].
    pub fn push_current_module(&self, name: &str) {
        self.current_module_stack
            .borrow_mut()
            .push(name.to_string());
    }

    /// Pop the module pushed by a matching [`Self::push_current_module`].
    pub fn pop_current_module(&self) {
        self.current_module_stack.borrow_mut().pop();
    }

    /// Name of the innermost module body currently being lowered, or `None` at
    /// top level. Used to bind `__module__` at macro-expansion time (#7919).
    pub fn current_module(&self) -> Option<String> {
        self.current_module_stack.borrow().last().cloned()
    }

    /// Get the current file path (for @__FILE__ macro).
    /// Returns "none" for REPL/unknown sources.
    pub fn get_current_file(&self) -> String {
        self.current_file
            .borrow()
            .clone()
            .unwrap_or_else(|| "none".to_string())
    }

    /// Get the current directory (for @__DIR__ macro).
    /// Returns "." for REPL/unknown sources.
    pub fn get_current_dir(&self) -> String {
        match &*self.current_file.borrow() {
            Some(path) => {
                let path = std::path::Path::new(path);
                path.parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string())
            }
            None => ".".to_string(),
        }
    }

    /// Generate a unique name for a lifted lambda function.
    pub fn next_lambda_name(&self) -> String {
        let mut counter = self.lambda_counter.borrow_mut();
        let name = format!("__lambda_{}", *counter);
        *counter += 1;
        name
    }

    /// Add a lifted function to the collection.
    pub fn add_lifted_function(&self, func: Function) {
        self.lifted_functions.borrow_mut().push(func);
    }

    /// Take all collected lifted functions.
    pub fn take_lifted_functions(&self) -> Vec<Function> {
        std::mem::take(&mut *self.lifted_functions.borrow_mut())
    }

    /// Number of lifted functions currently pending in this context.
    pub fn lifted_function_count(&self) -> usize {
        self.lifted_functions.borrow().len()
    }

    pub fn lifted_functions_from_index(&self, start: usize) -> Vec<Function> {
        self.lifted_functions
            .borrow()
            .iter()
            .skip(start)
            .cloned()
            .collect()
    }

    /// Take only lifted functions appended after `start`.
    ///
    /// Included files share the parent lowering context so macros and
    /// compile-time definitions remain visible across sequential `include`
    /// calls. This drains only the lambdas created by the included file,
    /// leaving any parent-file pending lambdas in place (Issue #7510).
    pub fn take_lifted_functions_from(&self, start: usize) -> Vec<Function> {
        let mut lifted = self.lifted_functions.borrow_mut();
        if start >= lifted.len() {
            return Vec::new();
        }
        lifted.split_off(start)
    }

    /// Record that a module has been imported via `using`.
    pub fn add_using(&self, module: &str) {
        self.usings.borrow_mut().insert(module.to_string());
    }

    fn snapshot_usings(&self) -> HashSet<String> {
        self.usings.borrow().clone()
    }

    fn restore_usings(&self, usings: HashSet<String>) {
        self.usings.replace(usings);
    }

    /// Check if a module has been imported via `using`.
    pub fn has_using(&self, module: &str) -> bool {
        self.usings.borrow().contains(module)
    }

    /// Register a user-defined macro (supports multiple arities).
    pub fn add_macro(&self, name: &str, macro_def: StoredMacroDef) {
        let mut macros = self.macros.borrow_mut();
        macros.entry(name.to_string()).or_default().push(macro_def);
    }

    pub fn add_macro_def(&self, macro_def: &MacroDef) {
        self.add_macro(
            &macro_def.name,
            StoredMacroDef {
                params: macro_def.params.clone(),
                param_types: vec![MacroParamType::Any; macro_def.params.len()],
                has_varargs: macro_def.has_varargs,
                body: macro_def.body.clone(),
                expansion_functions: vec![],
                expansion_structs: vec![],
                hygiene: None,
                span: macro_def.span,
            },
        );
    }

    /// Get a user-defined macro by name and arity.
    /// If arity is provided, returns the macro with matching arity.
    /// If no exact match, falls back to a varargs macro if available.
    pub fn get_macro_with_arity(&self, name: &str, arity: usize) -> Option<StoredMacroDef> {
        let macros = self.macros.borrow();
        if let Some(macro_defs) = macros.get(name) {
            // First, try to find an exact arity match
            for def in macro_defs {
                let expected_arity = if def.has_varargs {
                    def.params.len() - 1 // min args for varargs
                } else {
                    def.params.len()
                };
                let matches = if def.has_varargs {
                    arity >= expected_arity
                } else {
                    arity == expected_arity
                };
                if matches {
                    return Some(def.clone());
                }
            }
            // No exact match found
            None
        } else {
            None
        }
    }

    /// Get a user-defined macro by name, arity, and argument types.
    /// This enables Julia-style type-based macro dispatch.
    ///
    /// The dispatch algorithm:
    /// 1. First pass: find macros matching arity with exact type matches
    ///    (prioritize more specific types over Any)
    /// 2. Second pass: find macros matching arity with compatible types
    ///    (Any matches anything)
    pub fn get_macro_with_types(
        &self,
        name: &str,
        arg_types: &[MacroParamType],
    ) -> Option<StoredMacroDef> {
        let macros = self.macros.borrow();
        if let Some(macro_defs) = macros.get(name) {
            let arity = arg_types.len();

            // First pass: look for exact type matches (more specific takes priority)
            let mut best_match: Option<(StoredMacroDef, usize)> = None;

            for def in macro_defs {
                let expected_arity = if def.has_varargs {
                    def.params.len() - 1
                } else {
                    def.params.len()
                };

                let arity_matches = if def.has_varargs {
                    arity >= expected_arity
                } else {
                    arity == expected_arity
                };

                if !arity_matches {
                    continue;
                }

                // Check type compatibility and count specificity
                let (compatible, specificity) =
                    check_type_compatibility(&def.param_types, arg_types, def.has_varargs);

                if compatible {
                    match &best_match {
                        None => best_match = Some((def.clone(), specificity)),
                        Some((_, best_specificity)) if specificity > *best_specificity => {
                            best_match = Some((def.clone(), specificity));
                        }
                        _ => {}
                    }
                }
            }

            best_match.map(|(def, _)| def)
        } else {
            None
        }
    }

    /// Get a user-defined macro by name (returns first one, for backward compatibility).
    pub fn get_macro(&self, name: &str) -> Option<StoredMacroDef> {
        self.macros
            .borrow()
            .get(name)
            .and_then(|defs| defs.first().cloned())
    }

    /// Check if a macro with the given name is defined.
    pub fn has_macro(&self, name: &str) -> bool {
        self.macros.borrow().contains_key(name)
    }

    /// Record that `macro_name` is defined inside `module` whose member names are
    /// `members`. Used to qualify the macro's non-`esc` call targets at expansion
    /// time (Issue #7355 / #7350 A4).
    pub fn register_module_macro_hygiene(
        &self,
        macro_name: &str,
        module: &str,
        members: HashSet<String>,
        exports: HashSet<String>,
    ) {
        self.module_macro_hygiene.borrow_mut().insert(
            macro_name.to_string(),
            MacroHygieneInfo {
                module: module.to_string(),
                members,
                exports,
            },
        );
    }

    /// Begin converting `macro_name`'s expansion: if it is a module-defined macro,
    /// push its hygiene frame so member call targets get qualified. Returns whether
    /// a frame was pushed (caller must call [`Self::end_macro_hygiene`] iff true).
    pub fn begin_macro_hygiene(&self, macro_name: &str) -> bool {
        let frame = self
            .module_macro_hygiene
            .borrow()
            .get(macro_name)
            .map(|info| MacroHygieneFrame {
                module: info.module.clone(),
                members: info.members.clone(),
                esc_depth: 0,
            });
        match frame {
            Some(frame) => {
                self.macro_hygiene_stack.borrow_mut().push(frame);
                true
            }
            None => false,
        }
    }

    pub fn begin_macro_hygiene_frame(&self, module: &str, members: HashSet<String>) {
        self.macro_hygiene_stack
            .borrow_mut()
            .push(MacroHygieneFrame {
                module: module.to_string(),
                members,
                esc_depth: 0,
            });
    }

    pub fn macro_hygiene_entry(&self, macro_name: &str) -> Option<MacroHygieneInfo> {
        self.module_macro_hygiene.borrow().get(macro_name).cloned()
    }

    pub fn macro_hygiene_info_for_module(&self, module: &str) -> Option<MacroHygieneInfo> {
        self.module_macro_hygiene
            .borrow()
            .values()
            .find_map(|info| (info.module == module).then(|| info.clone()))
    }

    /// Pop the hygiene frame pushed by a matching [`Self::begin_macro_hygiene`].
    pub fn end_macro_hygiene(&self) {
        self.macro_hygiene_stack.borrow_mut().pop();
    }

    /// Enter an `esc(...)` subtree of the current macro expansion: identifiers
    /// inside resolve in the caller scope, so qualification is suppressed.
    pub fn enter_macro_esc(&self) {
        if let Some(frame) = self.macro_hygiene_stack.borrow_mut().last_mut() {
            frame.esc_depth += 1;
        }
    }

    /// Leave an `esc(...)` subtree.
    pub fn exit_macro_esc(&self) {
        if let Some(frame) = self.macro_hygiene_stack.borrow_mut().last_mut() {
            frame.esc_depth = frame.esc_depth.saturating_sub(1);
        }
    }

    /// If a module-defined macro is being expanded outside any `esc` and `name`
    /// is a member of its defining module, return that module name so the call
    /// target can be qualified `Module.name` (Issue #7355 / #7350 A4).
    pub fn qualify_module_macro_member(&self, name: &str) -> Option<String> {
        let stack = self.macro_hygiene_stack.borrow();
        let frame = stack.last()?;
        if frame.esc_depth == 0 && frame.members.contains(name) {
            Some(frame.module.clone())
        } else {
            None
        }
    }

    /// Register functions that may be called while evaluating macro bodies.
    pub fn add_compile_time_functions(&self, funcs: &[Function]) {
        self.compile_time_functions
            .borrow_mut()
            .extend(funcs.iter().cloned());
    }

    /// Snapshot functions visible to expansion-time macro execution.
    pub fn compile_time_functions(&self) -> Vec<Function> {
        self.compile_time_functions.borrow().clone()
    }

    pub fn add_compile_time_structs(&self, structs: &[StructDef]) {
        self.compile_time_structs
            .borrow_mut()
            .extend(structs.iter().cloned());
    }

    pub(crate) fn add_macro_expanded_struct(&self, struct_def: StructDef) {
        self.add_compile_time_structs(std::slice::from_ref(&struct_def));
        self.macro_expanded_structs.borrow_mut().push(struct_def);
    }

    pub(crate) fn take_macro_expanded_structs(&self) -> Vec<StructDef> {
        std::mem::take(&mut *self.macro_expanded_structs.borrow_mut())
    }

    pub fn add_compile_time_abstract_types(&self, types: &[AbstractTypeDef]) {
        self.compile_time_abstract_types
            .borrow_mut()
            .extend(types.iter().cloned());
    }

    pub fn add_compile_time_primitive_types(&self, types: &[PrimitiveTypeDef]) {
        self.compile_time_primitive_types
            .borrow_mut()
            .extend(types.iter().cloned());
    }

    /// Snapshot user type definitions visible to expansion-time macro execution.
    /// A bundled-package macro is expanded by compiling all `compile_time_functions`,
    /// so any user struct those functions touch must be present too (Issue #7272).
    pub fn compile_time_structs(&self) -> Vec<StructDef> {
        self.compile_time_structs.borrow().clone()
    }

    pub fn compile_time_abstract_types(&self) -> Vec<AbstractTypeDef> {
        self.compile_time_abstract_types.borrow().clone()
    }

    pub fn compile_time_primitive_types(&self) -> Vec<PrimitiveTypeDef> {
        self.compile_time_primitive_types.borrow().clone()
    }

    /// Get all imported module names.
    pub fn get_usings(&self) -> Vec<String> {
        self.usings.borrow().iter().cloned().collect()
    }
}

impl Default for LambdaContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Context for tracking include files during lowering.
/// Used to detect circular includes and resolve relative paths.
pub struct IncludeContext {
    /// Base directory for resolving relative paths.
    /// None means current working directory.
    base_dir: Option<PathBuf>,
    /// Set of already-included files (canonicalized paths) for circular detection.
    included_files: Rc<RefCell<HashSet<PathBuf>>>,
    /// Stack of include paths for error messages.
    include_stack: Rc<RefCell<Vec<PathBuf>>>,
}

impl IncludeContext {
    /// Create a new include context with an optional base directory.
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        Self {
            base_dir,
            included_files: Rc::new(RefCell::new(HashSet::new())),
            include_stack: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Create a child context for processing an included file.
    /// Shares the included_files and include_stack with the parent.
    pub fn child(&self, new_base_dir: Option<PathBuf>) -> Self {
        Self {
            base_dir: new_base_dir,
            included_files: Rc::clone(&self.included_files),
            include_stack: Rc::clone(&self.include_stack),
        }
    }

    /// Get the base directory for path resolution.
    pub fn base_dir(&self) -> Option<&PathBuf> {
        self.base_dir.as_ref()
    }

    /// Include a file: read, parse, and lower it.
    /// Returns the lowered Program if successful.
    pub fn include_file(&self, path: &str, span: Span) -> IncludeResult<IncludedContent> {
        self.include_file_with_macro_context(path, span, None)
    }

    /// Include a file using the caller's macro/lambda lowering context.
    ///
    /// Julia evaluates sequential includes in the same module scope. Sharing the
    /// lowering context makes macros and compile-time helper definitions from an
    /// earlier include visible to later includes in that same scope (Issue #7510).
    pub fn include_file_with_macro_context(
        &self,
        path: &str,
        span: Span,
        macro_ctx: Option<&LambdaContext>,
    ) -> IncludeResult<IncludedContent> {
        // 1. Resolve the path
        let resolved = resolve_include_path(path, self.base_dir.as_deref());

        // 2. Canonicalize for consistent circular detection
        let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

        // 3. Check for circular include
        if self.included_files.borrow().contains(&canonical) {
            let chain: Vec<String> = self
                .include_stack
                .borrow()
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            return Err(IncludeError::CircularInclude {
                path: canonical,
                include_chain: chain,
            });
        }

        // 4. Mark as included and push to stack
        self.included_files.borrow_mut().insert(canonical.clone());
        self.include_stack.borrow_mut().push(canonical.clone());

        // 5. Read the file
        let content = read_include_file(&resolved)?;

        // 6. Parse the content
        let mut parser = Parser::new().map_err(|e| IncludeError::ParseError {
            file_path: path.to_string(),
            message: e.to_string(),
        })?;
        let parse_outcome = parser
            .parse(&content)
            .map_err(|e| IncludeError::ParseError {
                file_path: path.to_string(),
                message: e.to_string(),
            })?;
        let exports =
            collect_source_file_exports(&parse_outcome).map_err(|e| IncludeError::LowerError {
                file_path: path.to_string(),
                message: e.to_string(),
            })?;

        // 7. Create child context with new base directory
        let child_base = resolved.parent().map(|p| p.to_path_buf());
        let child_ctx = self.child(child_base);

        // 8. Lower the parsed content
        let current_file = Some(resolved.to_string_lossy().to_string());
        let mut lowering =
            LoweringWithInclude::new_with_file(&content, child_ctx, Some(resolved.clone()));
        let program = match macro_ctx {
            Some(ctx) => ctx.with_current_file(current_file, || {
                lowering.lower_with_lambda_context(parse_outcome, ctx)
            }),
            None => {
                let lambda_ctx = LambdaContext::with_file(current_file);
                lowering.lower_with_lambda_context(parse_outcome, &lambda_ctx)
            }
        }
        .map_err(|e| IncludeError::LowerError {
            file_path: path.to_string(),
            message: e.to_string(),
        })?;

        // 9. Pop from stack (file is still in included_files to prevent re-include)
        self.include_stack.borrow_mut().pop();

        Ok(IncludedContent {
            program,
            file_path: resolved,
            span,
            exports,
        })
    }
}

impl Default for IncludeContext {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Content from an included file, ready to be merged into the parent program.
pub struct IncludedContent {
    /// The lowered program from the included file.
    pub program: Program,
    pub exports: Vec<String>,
    /// The resolved file path.
    pub file_path: PathBuf,
    /// The span of the include call in the parent file.
    pub span: Span,
}

impl IncludedContent {
    /// Merge the included content into a parent program.
    /// Functions, structs, abstract types, type aliases, modules, usings, and
    /// macros are added to the parent. Main statements are returned to be
    /// inlined at the include site.
    ///
    /// Macros must be threaded through too (Issue #6355): a bundled package like
    /// Plots defines `@animate`/`@gif` in an `include`d file (`api.jl`), and dropping
    /// them here made the macros invisible to `using Plots`.
    pub fn merge_into(
        self,
        functions: &mut Vec<Function>,
        structs: &mut Vec<StructDef>,
        abstract_types: &mut Vec<AbstractTypeDef>,
        primitive_types: &mut Vec<PrimitiveTypeDef>,
        type_aliases: &mut Vec<TypeAliasDef>,
        modules: &mut Vec<Module>,
        usings: &mut Vec<UsingImport>,
        macros: &mut Vec<MacroDef>,
        exports: Option<&mut Vec<String>>,
    ) -> Block {
        functions.extend(self.program.functions);
        structs.extend(self.program.structs);
        abstract_types.extend(self.program.abstract_types);
        primitive_types.extend(self.program.primitive_types);
        type_aliases.extend(self.program.type_aliases);
        modules.extend(self.program.modules);
        usings.extend(self.program.usings);
        macros.extend(self.program.macros);
        if let Some(parent_exports) = exports {
            parent_exports.extend(self.exports);
        }
        self.program.main
    }
}

fn collect_source_file_exports(parse_outcome: &ParseOutcome) -> LowerResult<Vec<String>> {
    let ParseOutcome::Rust(parsed) = parse_outcome;
    let walker = CstWalker::new(parsed.source());
    let root = Node::new(parsed.root(), parsed.source());
    let mut exports = Vec::new();
    for child in walker.named_children(&root) {
        if walker.kind(&child) == NodeKind::ExportStatement {
            exports.extend(lower_export_statement(&walker, child)?);
        }
    }
    Ok(exports)
}

pub struct Lowering<'a> {
    _source: &'a str,
    walker: CstWalker<'a>,
    /// Store the parsed source so it lives long enough for Node references
    parsed_rust: Option<crate::parser::RustParsedSource>,
    initial_usings: Vec<String>,
}

impl<'a> Lowering<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            _source: source,
            walker: CstWalker::new(source),
            parsed_rust: None,
            initial_usings: Vec::new(),
        }
    }

    pub fn new_with_usings(source: &'a str, usings: &[UsingImport]) -> Self {
        let mut lowering = Self::new(source);
        lowering.initial_usings = usings.iter().map(|using| using.module.clone()).collect();
        lowering
    }

    pub fn lower(&mut self, parse_outcome: ParseOutcome) -> LowerResult<Program> {
        let ParseOutcome::Rust(parsed) = parse_outcome;
        self.parsed_rust = Some(parsed);
        // SAFETY: We know parsed is Some because we just set it, and it will live
        // as long as self.
        let parsed_ref = self.parsed_rust.as_ref().unwrap();
        let root = Node::new(parsed_ref.root(), parsed_ref.source());
        self.lower_source_file(root)
    }

    /// Lower a source file (or module body) to a `Program`. Wraps the inner
    /// lowering with a type-alias scope (Issue #5055): user-defined aliases are
    /// pre-registered for this pass and the prior alias table is restored on
    /// exit, so a nested pass (e.g. the stdlib load triggered by `using Test`)
    /// cannot destroy the enclosing program's aliases.
    fn lower_source_file(&self, node: Node<'a>) -> LowerResult<Program> {
        let scope = type_alias::snapshot();
        prescan_and_register_type_aliases(&self.walker, node);
        let result = self.lower_source_file_inner(node);
        scope.restore();
        result
    }

    fn lower_source_file_inner(&self, node: Node<'a>) -> LowerResult<Program> {
        let mut abstract_types = Vec::new();
        let mut primitive_types = Vec::new();
        let mut type_aliases = Vec::new();
        let mut structs = Vec::new();
        let mut functions = Vec::new();
        let mut modules = Vec::new();
        let mut usings = Vec::new();
        let mut macros = Vec::new();
        let mut main_stmts = Vec::new();

        // Create lambda context for collecting lifted anonymous functions
        let lambda_ctx = LambdaContext::new();
        for module in &self.initial_usings {
            lambda_ctx.add_using(module);
            ensure_stdlib_macros_loaded(module);
            ensure_bundled_package_macros_loaded(module);
        }

        let children = self.walker.named_children(&node);
        for (idx, child) in children.iter().enumerate() {
            let kind = self.walker.kind(child);

            // Issue #4357: skip top-level string literals that precede a
            // definition — these are docstrings in Julia (`"""doc""" function f end`
            // ≡ `@doc "doc" function f end`) and should not become value-producing
            // main statements. Otherwise the final docstring in the concatenated
            // Base sources leaks as the return value of any top-level input whose
            // main block is empty (e.g. `using LinearAlgebra`).
            if matches!(kind, NodeKind::StringLiteral)
                && is_top_level_docstring(&self.walker, &children, idx)
            {
                continue;
            }

            let child = *child;
            match kind {
                // Skip comments
                NodeKind::LineComment | NodeKind::BlockComment => continue,
                NodeKind::AbstractDefinition => {
                    let abstract_def = abstract_::lower_abstract_definition(&self.walker, child)?;
                    lambda_ctx.add_compile_time_abstract_types(std::slice::from_ref(&abstract_def));
                    abstract_types.push(abstract_def);
                }
                NodeKind::PrimitiveDefinition => {
                    let primitive_def = primitive::lower_primitive_definition(&self.walker, child)?;
                    lambda_ctx
                        .add_compile_time_primitive_types(std::slice::from_ref(&primitive_def));
                    primitive_types.push(primitive_def);
                }
                NodeKind::StructDefinition | NodeKind::MutableStructDefinition => {
                    let struct_def = struct_::lower_struct_definition(&self.walker, child)?;
                    lambda_ctx.add_compile_time_structs(std::slice::from_ref(&struct_def));
                    structs.push(struct_def);
                }
                NodeKind::FunctionDefinition => {
                    let funcs = lower_function_all_with_macro_ctx_if_needed(
                        &self.walker,
                        child,
                        &lambda_ctx,
                    )?;
                    reject_macro_expanded_structs_in_non_toplevel(
                        &lambda_ctx,
                        self.walker.span(&child),
                    )?;
                    lambda_ctx.add_compile_time_functions(&funcs);
                    functions.extend(funcs);
                }
                NodeKind::ShortFunctionDefinition => {
                    // Operator method definitions: *(x, y) = expr
                    let func = lower_operator_method_with_macro_ctx_if_needed(
                        &self.walker,
                        child,
                        &lambda_ctx,
                    )?;
                    reject_macro_expanded_structs_in_non_toplevel(
                        &lambda_ctx,
                        self.walker.span(&child),
                    )?;
                    lambda_ctx.add_compile_time_functions(std::slice::from_ref(&func));
                    functions.push(func);
                }
                NodeKind::MacroDefinition => {
                    let lifted_start = lambda_ctx.lifted_function_count();
                    let (macro_def, param_types) =
                        lower_macro_definition(&self.walker, child, Some(&lambda_ctx))?;
                    let macro_lambdas = lambda_ctx.lifted_functions_from_index(lifted_start);
                    lambda_ctx.add_compile_time_functions(&macro_lambdas);
                    // Register macro in context for expansion during lowering
                    lambda_ctx.add_macro(
                        &macro_def.name,
                        StoredMacroDef {
                            params: macro_def.params.clone(),
                            param_types,
                            has_varargs: macro_def.has_varargs,
                            body: macro_def.body.clone(),
                            expansion_functions: vec![],
                            expansion_structs: vec![],
                            hygiene: None,
                            span: macro_def.span,
                        },
                    );
                    macros.push(macro_def);
                }
                NodeKind::ModuleDefinition => {
                    let module = lower_module_definition(
                        &self.walker,
                        child,
                        false,
                        None,
                        Some(&lambda_ctx),
                    )?;
                    modules.push(module);
                }
                NodeKind::BaremoduleDefinition => {
                    let module = lower_module_definition(
                        &self.walker,
                        child,
                        true,
                        None,
                        Some(&lambda_ctx),
                    )?;
                    modules.push(module);
                }
                NodeKind::UsingStatement | NodeKind::ImportStatement => {
                    // using Module or import Module (possibly comma-separated, e.g.
                    // `using A, B` → one UsingImport per module).
                    for using_import in lower_using_statement(&self.walker, child)? {
                        // Record in lambda context for macro availability checks
                        lambda_ctx.add_using(&using_import.module);
                        // Load stdlib module macros early so they can be expanded
                        ensure_stdlib_macros_loaded(&using_import.module);
                        // Same for embedded bundled packages (e.g. Plots' @animate/@gif).
                        ensure_bundled_package_macros_loaded(&using_import.module);
                        // Bind any `... as ...` renames at the point of import so the
                        // alias name resolves to the imported entity (Issue #8117).
                        main_stmts.extend(using_import_alias_stmts(&using_import));
                        usings.push(using_import);
                    }
                }
                NodeKind::Assignment
                    if function::is_short_function_definition(&self.walker, child) =>
                {
                    // Short function definition: f(x) = expr
                    let funcs = lower_short_function_all_with_macro_ctx_if_needed(
                        &self.walker,
                        child,
                        &lambda_ctx,
                    )?;
                    reject_macro_expanded_structs_in_non_toplevel(
                        &lambda_ctx,
                        self.walker.span(&child),
                    )?;
                    lambda_ctx.add_compile_time_functions(&funcs);
                    functions.extend(funcs);
                }
                NodeKind::Assignment if function::is_lambda_assignment(&self.walker, child) => {
                    // Lambda assignment: f = x -> expr
                    // May return multiple methods: the main lambda plus reduced-arity
                    // default-arg stubs for `(x, d=2) -> ...` (Issue #8047).
                    let funcs = function::lower_lambda_assignment(&self.walker, child)?;
                    lambda_ctx.add_compile_time_functions(&funcs);
                    functions.extend(funcs);
                }
                NodeKind::Assignment
                    if stmt::try_extract_type_alias_from_assignment(&self.walker, child)
                        .is_some() =>
                {
                    // Issue #5055: a plain (non-`const`) type-alias definition
                    // such as `MyVec{T} = Vector{T}` or `IntVec = Vector{Int}`.
                    // Already registered by the pre-scan; collect it and emit no
                    // runtime statement (the binding is purely a type alias).
                    if let Some(type_alias) =
                        stmt::try_extract_type_alias_from_assignment(&self.walker, child)
                    {
                        type_aliases.push(type_alias);
                    }
                }
                NodeKind::MacroCall if is_kwdef_macro(&self.walker, child) => {
                    // @kwdef struct ... end - expand to struct def + constructor
                    let (struct_def, ctor_func) = expand_kwdef_macro(&self.walker, child)?;
                    lambda_ctx.add_compile_time_structs(std::slice::from_ref(&struct_def));
                    structs.push(struct_def);
                    functions.push(ctor_func);
                }
                NodeKind::ConstStatement => {
                    // Check if this is a type alias definition
                    if let Some(type_alias) = stmt::try_extract_type_alias(&self.walker, child) {
                        type_aliases.push(type_alias);
                    }
                    // Always lower const statements so the variable is accessible at runtime
                    let stmt = stmt::lower_stmt_with_ctx(&self.walker, child, &lambda_ctx)?;
                    drain_macro_expanded_structs(&lambda_ctx, &mut structs);
                    main_stmts.push(stmt);
                }
                _ => {
                    // Use context-aware lowering to handle inline lambdas
                    let stmt = stmt::lower_stmt_with_ctx(&self.walker, child, &lambda_ctx)?;
                    drain_macro_expanded_structs(&lambda_ctx, &mut structs);
                    match extract_top_level_function_defs(stmt) {
                        Ok(funcs) => functions.extend(funcs),
                        Err(stmt) => main_stmts.push(*stmt),
                    }
                }
            }
        }

        // Collect lifted lambda functions
        let lifted_functions = lambda_ctx.take_lifted_functions();
        functions.extend(lifted_functions);

        // Box scalar locals that are captured by a closure and reassigned, so the
        // closure observes the new value (Julia cell semantics, Issue #6262).
        closure_box::box_captured_reassigned_locals(&mut functions, &mut main_stmts);

        let span = self.walker.span(&node);
        Ok(Program {
            abstract_types,
            primitive_types,
            type_aliases,
            structs,
            functions,
            base_function_count: 0,
            modules,
            usings,
            macros,
            enums: vec![],
            main: Block {
                stmts: main_stmts,
                span,
            },
        })
    }
}

/// Extract the string path from an `include("path")` CallExpression node.
/// Returns `Ok(Some(path))` if the node is a valid include() call,
/// `Ok(None)` if it is not an include() call, or an error if the call
/// is malformed (e.g., dynamic path or missing argument).
fn try_extract_include_path<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<String>> {
    let children = walker.named_children(&node);
    let callee = match children.first() {
        Some(n) => n,
        None => return Ok(None),
    };
    if walker.kind(callee) != NodeKind::Identifier {
        return Ok(None);
    }
    if walker.text(callee) != "include" {
        return Ok(None);
    }
    let span = walker.span(&node);
    let args_node = children
        .iter()
        .find(|n| walker.kind(n) == NodeKind::ArgumentList);
    let path = if let Some(args) = args_node {
        let arg_children = walker.named_children(args);
        if let Some(first_arg) = arg_children.first() {
            if walker.kind(first_arg) == NodeKind::StringLiteral {
                walker.text(first_arg).trim_matches('"').to_string()
            } else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::IncludeCall("<dynamic path>".to_string()),
                    span,
                )
                .with_hint("include() requires a string literal path"));
            }
        } else {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::IncludeCall("<missing argument>".to_string()),
                span,
            )
            .with_hint("include() requires a path argument"));
        }
    } else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::IncludeCall("<no arguments>".to_string()),
            span,
        )
        .with_hint("include() requires a path argument"));
    };
    Ok(Some(path))
}

/// Lower a module definition: `module Name ... end` or `baremodule Name ... end`
fn lower_module_definition<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    is_bare: bool,
    include_ctx: Option<&IncludeContext>,
    // Macros defined inside a module are registered here so that they can be
    // resolved at the call site (Issue #7355 / #7350 A4). sjulia uses a flat
    // namespace — module functions are already hoisted globally — so a
    // module-defined macro must likewise be callable both unqualified (after
    // `using .M`) and qualified (`M.@m(...)`), and the parser drops the module
    // prefix in either case, leaving a bare macro name to look up.
    macro_ctx: Option<&LambdaContext>,
) -> LowerResult<Module> {
    let span = walker.span(&node);

    // Get module name from the 'name' field
    let name_node = walker.child_by_field(&node, "name").ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::ModuleDefinition, span)
            .with_hint("module definition must have a name")
    })?;

    // Only support simple identifier names (not interpolation)
    let name = match walker.kind(&name_node) {
        NodeKind::Identifier => walker.text(&name_node).to_string(),
        _ => {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::ModuleDefinition, span).with_hint(
                    "module name must be a simple identifier (interpolation not supported)",
                ),
            )
        }
    };

    // Get block (body of the module) - it's a child, not a field
    let body_node = walker
        .named_children(&node)
        .into_iter()
        .find(|n| walker.kind(n) == NodeKind::Block);

    // Extract functions, structs, abstract types, type aliases, exports, submodules, using statements, macros, and statements from the module body
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut abstract_types = Vec::new();
    let mut primitive_types = Vec::new();
    let mut type_aliases = Vec::new();
    let mut submodules = Vec::new();
    let mut usings = Vec::new();
    let mut macros = Vec::new();
    let mut exports = Vec::new();
    let mut publics = Vec::new();
    let mut body_stmts = Vec::new();
    let previous_macro_ctx_usings = macro_ctx.map(LambdaContext::snapshot_usings);

    // Track the enclosing module so a macro expanded inside this body binds
    // `__module__` to this module rather than the hard-coded `Main` (Issue
    // #7919). Only the macro-context path consumes this; pop after the loop.
    if let Some(ctx) = macro_ctx {
        ctx.push_current_module(&name);
    }

    if let Some(block_node) = body_node {
        let children = walker.named_children(&block_node);
        for (idx, child) in children.iter().enumerate() {
            let kind = walker.kind(child);

            // Issue #4357: drop top-level docstrings inside module bodies too.
            // Same rule as `lower_source_file` — a `StringLiteral` immediately
            // followed by a definition is Julia documentation, not a value.
            if matches!(kind, NodeKind::StringLiteral)
                && is_top_level_docstring(walker, &children, idx)
            {
                continue;
            }

            let child = *child;
            match kind {
                // Skip comments
                NodeKind::LineComment | NodeKind::BlockComment => continue,
                // Handle struct definitions
                NodeKind::StructDefinition | NodeKind::MutableStructDefinition => {
                    let struct_def = struct_::lower_struct_definition(walker, child)?;
                    if let Some(ctx) = macro_ctx {
                        ctx.add_compile_time_structs(std::slice::from_ref(&struct_def));
                    }
                    structs.push(struct_def);
                }
                // Handle abstract type definitions
                NodeKind::AbstractDefinition => {
                    let abstract_def = abstract_::lower_abstract_definition(walker, child)?;
                    if let Some(ctx) = macro_ctx {
                        ctx.add_compile_time_abstract_types(std::slice::from_ref(&abstract_def));
                    }
                    abstract_types.push(abstract_def);
                }
                // Handle primitive type definitions
                NodeKind::PrimitiveDefinition => {
                    let primitive_def = primitive::lower_primitive_definition(walker, child)?;
                    if let Some(ctx) = macro_ctx {
                        ctx.add_compile_time_primitive_types(std::slice::from_ref(&primitive_def));
                    }
                    primitive_types.push(primitive_def);
                }
                NodeKind::FunctionDefinition => {
                    let funcs = match macro_ctx {
                        Some(ctx) => {
                            lower_function_all_with_macro_ctx_if_needed(walker, child, ctx)?
                        }
                        None => function::lower_function_all(walker, child)?,
                    };
                    if let Some(ctx) = macro_ctx {
                        reject_macro_expanded_structs_in_non_toplevel(ctx, walker.span(&child))?;
                        ctx.add_compile_time_functions(&funcs);
                    }
                    functions.extend(funcs);
                }
                NodeKind::ShortFunctionDefinition => {
                    // Operator method definitions: *(x, y) = expr
                    let func = match macro_ctx {
                        Some(ctx) => {
                            lower_operator_method_with_macro_ctx_if_needed(walker, child, ctx)?
                        }
                        None => function::lower_operator_method(walker, child)?,
                    };
                    if let Some(ctx) = macro_ctx {
                        reject_macro_expanded_structs_in_non_toplevel(ctx, walker.span(&child))?;
                        ctx.add_compile_time_functions(std::slice::from_ref(&func));
                    }
                    functions.push(func);
                }
                NodeKind::ModuleDefinition => {
                    // Nested module: recursively lower. Pass the macro context
                    // through so submodule macros are also resolvable (flat
                    // namespace, Issue #7355).
                    let submodule =
                        lower_module_definition(walker, child, false, include_ctx, macro_ctx)?;
                    submodules.push(submodule);
                }
                NodeKind::BaremoduleDefinition => {
                    // Nested baremodule: recursively lower
                    let submodule =
                        lower_module_definition(walker, child, true, include_ctx, macro_ctx)?;
                    submodules.push(submodule);
                }
                NodeKind::UsingStatement | NodeKind::ImportStatement => {
                    // using Module or import Module (possibly comma-separated, e.g.
                    // `using A, B` → one UsingImport per module).
                    for using_import in lower_using_statement(walker, child)? {
                        if let Some(ctx) = macro_ctx {
                            ctx.add_using(&using_import.module);
                            ensure_stdlib_macros_loaded(&using_import.module);
                            ensure_bundled_package_macros_loaded(&using_import.module);
                        }
                        // Bind any `... as ...` renames at the point of import so the
                        // alias name resolves to the imported entity (Issue #8117).
                        body_stmts.extend(using_import_alias_stmts(&using_import));
                        usings.push(using_import);
                    }
                }
                NodeKind::ExportStatement => {
                    // export func1, func2, ...
                    let export_names = lower_export_statement(walker, child)?;
                    exports.extend(export_names.clone());
                    body_stmts.push(Stmt::Export {
                        names: export_names,
                        span: walker.span(&child),
                    });
                }
                NodeKind::PublicStatement => {
                    // public func1, func2, ... (Julia 1.11+)
                    let public_names = lower_public_statement(walker, child)?;
                    publics.extend(public_names);
                }
                NodeKind::MacroDefinition => {
                    // Macro definition within module
                    let lifted_start = macro_ctx.map(LambdaContext::lifted_function_count);
                    let (macro_def, param_types) =
                        lower_macro_definition(walker, child, macro_ctx)?;
                    // Register into the call-site macro context so the macro is
                    // resolvable from outside the module (Issue #7355 / #7350 A4).
                    if let Some(ctx) = macro_ctx {
                        if let Some(start) = lifted_start {
                            let macro_lambdas = ctx.lifted_functions_from_index(start);
                            ctx.add_compile_time_functions(&macro_lambdas);
                        }
                        ctx.add_macro(
                            &macro_def.name,
                            StoredMacroDef {
                                params: macro_def.params.clone(),
                                param_types,
                                has_varargs: macro_def.has_varargs,
                                body: macro_def.body.clone(),
                                expansion_functions: vec![],
                                expansion_structs: vec![],
                                hygiene: None,
                                span: macro_def.span,
                            },
                        );
                    }
                    macros.push(macro_def);
                }
                NodeKind::Assignment if function::is_short_function_definition(walker, child) => {
                    // Short function definition: f(x) = expr
                    let funcs = match macro_ctx {
                        Some(ctx) => {
                            lower_short_function_all_with_macro_ctx_if_needed(walker, child, ctx)?
                        }
                        None => function::lower_short_function_all(walker, child)?,
                    };
                    if let Some(ctx) = macro_ctx {
                        reject_macro_expanded_structs_in_non_toplevel(ctx, walker.span(&child))?;
                        ctx.add_compile_time_functions(&funcs);
                    }
                    functions.extend(funcs);
                }
                NodeKind::Assignment if function::is_lambda_assignment(walker, child) => {
                    // Lambda assignment: f = x -> expr
                    // May return multiple methods: the main lambda plus reduced-arity
                    // default-arg stubs for `(x, d=2) -> ...` (Issue #8047).
                    let funcs = function::lower_lambda_assignment(walker, child)?;
                    if let Some(ctx) = macro_ctx {
                        ctx.add_compile_time_functions(&funcs);
                    }
                    functions.extend(funcs);
                }
                NodeKind::Assignment
                    if stmt::try_extract_type_alias_from_assignment(walker, child).is_some() =>
                {
                    // Issue #5055: a plain (non-`const`) type-alias definition.
                    // Already registered by the pre-scan; collect it and emit no
                    // runtime statement.
                    if let Some(type_alias) =
                        stmt::try_extract_type_alias_from_assignment(walker, child)
                    {
                        type_aliases.push(type_alias);
                    }
                }
                NodeKind::ConstStatement => {
                    // Check if this is a type alias definition
                    if let Some(type_alias) = stmt::try_extract_type_alias(walker, child) {
                        type_aliases.push(type_alias);
                    }
                    // Always lower const statements so the variable is accessible at runtime
                    let stmt = lower_stmt_with_macro_ctx_if_needed(walker, child, macro_ctx)?;
                    if let Some(ctx) = macro_ctx {
                        drain_macro_expanded_structs(ctx, &mut structs);
                    }
                    body_stmts.push(stmt);
                }
                NodeKind::CallExpression => {
                    // Handle include() calls inside the module body when an include
                    // context is available (e.g. package loader, LoweringWithInclude).
                    if let Some(ctx) = include_ctx {
                        let span = walker.span(&child);
                        match try_extract_include_path(walker, child)? {
                            Some(path) => {
                                let included = ctx
                                    .include_file_with_macro_context(&path, span, macro_ctx)
                                    .map_err(|e| {
                                        UnsupportedFeature::new(
                                            UnsupportedFeatureKind::Other(e.to_string()),
                                            span,
                                        )
                                    })?;
                                if let Some(parent_ctx) = macro_ctx {
                                    parent_ctx.add_compile_time_abstract_types(
                                        &included.program.abstract_types,
                                    );
                                    parent_ctx.add_compile_time_primitive_types(
                                        &included.program.primitive_types,
                                    );
                                    parent_ctx.add_compile_time_structs(&included.program.structs);
                                    parent_ctx
                                        .add_compile_time_functions(&included.program.functions);
                                    for macro_def in &included.program.macros {
                                        parent_ctx.add_macro_def(macro_def);
                                    }
                                }
                                let inline_block = included.merge_into(
                                    &mut functions,
                                    &mut structs,
                                    &mut abstract_types,
                                    &mut primitive_types,
                                    &mut type_aliases,
                                    &mut submodules,
                                    &mut usings,
                                    &mut macros,
                                    Some(&mut exports),
                                );
                                if let Some(parent_ctx) = macro_ctx {
                                    // Macros expanded while lowering an included
                                    // file may store struct metadata on the
                                    // parent module context. Drain after merging
                                    // so definitions like `@attributes mutable
                                    // struct ... end` become module structs
                                    // (Issue #7945).
                                    drain_macro_expanded_structs(parent_ctx, &mut structs);
                                }
                                body_stmts.extend(inline_block.stmts);
                            }
                            None => {
                                let stmt =
                                    lower_stmt_with_macro_ctx_if_needed(walker, child, macro_ctx)?;
                                if let Some(ctx) = macro_ctx {
                                    drain_macro_expanded_structs(ctx, &mut structs);
                                }
                                match extract_top_level_function_defs(stmt) {
                                    Ok(funcs) => functions.extend(funcs),
                                    Err(stmt) => body_stmts.push(*stmt),
                                }
                            }
                        }
                    } else {
                        let stmt = lower_stmt_with_macro_ctx_if_needed(walker, child, macro_ctx)?;
                        if let Some(ctx) = macro_ctx {
                            drain_macro_expanded_structs(ctx, &mut structs);
                        }
                        match extract_top_level_function_defs(stmt) {
                            Ok(funcs) => functions.extend(funcs),
                            Err(stmt) => body_stmts.push(*stmt),
                        }
                    }
                }
                _ => {
                    let stmt = lower_stmt_with_macro_ctx_if_needed(walker, child, macro_ctx)?;
                    if let Some(ctx) = macro_ctx {
                        drain_macro_expanded_structs(ctx, &mut structs);
                    }
                    match extract_top_level_function_defs(stmt) {
                        Ok(funcs) => functions.extend(funcs),
                        Err(stmt) => body_stmts.push(*stmt),
                    }
                }
            }
        }
    }

    if let Some(ctx) = macro_ctx {
        ctx.pop_current_module();
    }

    let body = Block {
        stmts: body_stmts,
        span,
    };
    collect_module_body_exports(&body, &mut exports, &name);

    // Register hygiene info for this module's macros now that the full set of
    // member names and real exports is known (Issue #7355 / #7350 A4). A
    // non-`esc` call target in a module-defined macro that names a module
    // member is qualified `M.name` at expansion time so unexported members
    // resolve in the defining module. The export set is kept separate so
    // macro-expanded `$__module__` literals do not expose unexported hygiene
    // members through `names($__module__)`.
    if let Some(ctx) = macro_ctx {
        if !macros.is_empty() {
            let mut members: HashSet<String> = HashSet::new();
            members.extend(
                functions
                    .iter()
                    .filter(|f| !f.is_base_extension)
                    .map(|f| f.name.clone()),
            );
            members.extend(structs.iter().map(|s| s.name.clone()));
            members.extend(abstract_types.iter().map(|a| a.name.clone()));
            members.extend(primitive_types.iter().map(|p| p.name.clone()));
            members.extend(type_aliases.iter().map(|t| t.name.clone()));
            let export_set: HashSet<String> = exports.iter().cloned().collect();
            for macro_def in &macros {
                ctx.register_module_macro_hygiene(
                    &macro_def.name,
                    &name,
                    members.clone(),
                    export_set.clone(),
                );
            }
        }
    }

    let module = Module {
        name,
        is_bare,
        functions,
        structs,
        abstract_types,
        primitive_types,
        type_aliases,
        submodules,
        usings,
        macros,
        exports,
        publics,
        body,
        span,
    };

    if let (Some(ctx), Some(previous_usings)) = (macro_ctx, previous_macro_ctx_usings) {
        ctx.restore_usings(previous_usings);
    }

    Ok(module)
}

/// Lower an export statement: `export func1, func2, ...`
/// Returns the list of exported names.
pub(crate) fn lower_export_statement<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<String>> {
    let mut names = Vec::new();

    // Collect all identifier children as exported names
    for child in walker.named_children(&node) {
        if walker.kind(&child) == NodeKind::Identifier {
            names.push(walker.text(&child).to_string());
        }
    }

    Ok(names)
}

fn collect_module_body_exports(block: &Block, exports: &mut Vec<String>, module_name: &str) {
    let mut known_exports: HashSet<String> = HashSet::new();
    let mut emitted_exports: HashSet<String> = exports.iter().cloned().collect();
    known_exports.insert(module_name.to_string());
    collect_module_body_exports_inner(
        block,
        exports,
        &mut known_exports,
        &mut emitted_exports,
        module_name,
    );
}

fn collect_module_body_exports_inner(
    block: &Block,
    exports: &mut Vec<String>,
    known_exports: &mut HashSet<String>,
    emitted_exports: &mut HashSet<String>,
    module_name: &str,
) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Export { names, .. } => {
                for name in names {
                    known_exports.insert(name.clone());
                    if emitted_exports.insert(name.clone()) {
                        exports.push(name.clone());
                    }
                }
            }
            Stmt::Block(inner) => {
                collect_module_body_exports_inner(
                    inner,
                    exports,
                    known_exports,
                    emitted_exports,
                    module_name,
                );
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => match eval_module_export_condition(condition, known_exports, module_name) {
                Some(true) => collect_module_body_exports_inner(
                    then_branch,
                    exports,
                    known_exports,
                    emitted_exports,
                    module_name,
                ),
                Some(false) => {
                    if let Some(else_branch) = else_branch {
                        collect_module_body_exports_inner(
                            else_branch,
                            exports,
                            known_exports,
                            emitted_exports,
                            module_name,
                        );
                    }
                }
                None => {}
            },
            _ => {}
        }
    }
}

fn eval_module_export_condition(
    expr: &Expr,
    known_exports: &HashSet<String>,
    module_name: &str,
) -> Option<bool> {
    match expr {
        Expr::Literal(Literal::Bool(value), _) => Some(*value),
        Expr::Var(name, _) if name == "true" => Some(true),
        Expr::Var(name, _) if name == "false" => Some(false),
        Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand,
            ..
        } => eval_module_export_condition(operand, known_exports, module_name).map(|value| !value),
        Expr::Call { function, args, .. }
            if matches!(function.as_str(), "in" | "∈") && args.len() == 2 =>
        {
            eval_symbol_in_module_names(&args[0], &args[1], known_exports, module_name)
        }
        Expr::Call { function, args, .. }
            if matches!(function.as_str(), "∉") && args.len() == 2 =>
        {
            eval_symbol_in_module_names(&args[0], &args[1], known_exports, module_name)
                .map(|value| !value)
        }
        Expr::Builtin {
            name: BuiltinOp::In,
            args,
            ..
        } if args.len() == 2 => {
            eval_symbol_in_module_names(&args[0], &args[1], known_exports, module_name)
        }
        _ => None,
    }
}

fn eval_symbol_in_module_names(
    needle: &Expr,
    haystack: &Expr,
    known_exports: &HashSet<String>,
    module_name: &str,
) -> Option<bool> {
    let symbol = expr_symbol_name(needle)?;
    if names_call_module_name(haystack)? != module_name {
        return None;
    }
    Some(known_exports.contains(symbol))
}

fn expr_symbol_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Literal(Literal::Symbol(name), _) => Some(name),
        Expr::Literal(Literal::QuoteNode(inner), _) => match inner.as_ref() {
            Literal::Symbol(name) => Some(name),
            _ => None,
        },
        Expr::QuoteLiteral { constructor, .. } => expr_symbol_name(constructor),
        Expr::Builtin {
            name: BuiltinOp::SymbolNew,
            args,
            ..
        } if args.len() == 1 => match &args[0] {
            Expr::Literal(Literal::Str(name), _) => Some(name),
            _ => None,
        },
        _ => None,
    }
}

fn names_call_module_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Call { function, args, .. } if function == "names" && args.len() == 1 => {
            expr_module_name(&args[0])
        }
        _ => None,
    }
}

fn expr_module_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Literal(Literal::Module(name), _) => Some(name),
        Expr::Var(name, _) => Some(name),
        _ => None,
    }
}

/// Lower a public statement: `public func1, func2, ...` (Julia 1.11+)
/// Returns the list of public names.
fn lower_public_statement<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Vec<String>> {
    let mut names = Vec::new();

    // Collect all identifier children as public names
    for child in walker.named_children(&node) {
        if walker.kind(&child) == NodeKind::Identifier {
            names.push(walker.text(&child).to_string());
        }
    }

    Ok(names)
}

/// Lower a using/import statement: `using Module` or `using Module: func1, func2`
/// Also handles relative imports: `using .Module` (references user-defined modules)
/// Lower a `using` / `import` statement into one `UsingImport` per imported module.
///
/// Returns a vector because Julia allows a comma-separated list — `using A, B` is
/// `using A; using B` (and `import A, B` likewise). The CST shape is
/// `UsingStatement > import_list > import_path+`, so each `import_path` child is
/// lowered independently via [`lower_one_import_path`]. A single-module `using A`
/// just yields a one-element vector. (Previously only the whole `import_list` was
/// read, so `using A, B` produced a single bogus module named `"A, B"`.)
fn lower_using_statement<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<UsingImport>> {
    let span = walker.span(&node);

    // The statement's named child is the `import_list`; its named children are the
    // individual `import_path`s. Fall back to treating the statement's own named
    // children as paths if no `import_list` wrapper is present (defensive).
    let named = walker.named_children(&node);
    let paths: Vec<Node<'a>> = match named.first() {
        Some(first) if first.kind() == "import_list" => walker.named_children(first),
        _ => named,
    };

    if paths.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                .with_hint("using statement must specify a module name"),
        );
    }

    let is_import_statement = walker.kind(&node) == NodeKind::ImportStatement;
    let mut imports = Vec::with_capacity(paths.len());
    for path in paths {
        imports.push(lower_one_import_path(
            walker,
            path,
            span,
            is_import_statement,
        )?);
    }
    Ok(imports)
}

/// Split a single import entry of the form `name as alias` into its parts,
/// returning `None` when there is no `as` rename. The `as` is matched as a
/// whitespace-delimited keyword token, so identifiers that merely contain the
/// letters `as` (e.g. `mass`) are left untouched (Issue #8117).
fn split_as_rename(entry: &str) -> Option<(String, String)> {
    let tokens: Vec<&str> = entry.split_whitespace().collect();
    if tokens.len() == 3 && tokens[1] == "as" {
        Some((tokens[0].to_string(), tokens[2].to_string()))
    } else {
        None
    }
}

/// Lower a single `import_path` node (one comma-separated entry of a `using` /
/// `import` statement) into a `UsingImport`.
///
/// Handles plain (`A`), scoped (`Base.Sort`), relative (`.A`), selective
/// (`A: f, g`), and renaming (`A as B`, `A: f as g`) forms. Selective imports
/// are detected by a `:` in the path text; scoped names like `Base.Sort` have
/// no `:` and are kept as the full module path. Renames (`... as ...`) are
/// recorded in `alias_bindings` so a later pass can bind the alias name to the
/// imported entity (Issue #8117).
fn lower_one_import_path<'a>(
    walker: &CstWalker<'a>,
    path: Node<'a>,
    span: Span,
    is_import_statement: bool,
) -> LowerResult<UsingImport> {
    // The path text carries no leading `using`/`import` keyword (that lives on the
    // enclosing statement node), e.g. `"Plots"`, `"Base.Sort"`, `"Plots: plot"`.
    let path_text = walker.text(&path);

    // Selective import: `Module: func1, func2` (each entry may be `f as g`).
    if let Some(colon_pos) = path_text.find(':') {
        let before_colon = path_text[..colon_pos].trim();
        let relative_level = before_colon.chars().take_while(|c| *c == '.').count();
        let is_relative = relative_level > 0;
        let module_name = before_colon.trim_start_matches('.').to_string();

        if module_name.is_empty() {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                    .with_hint("using statement must specify a module name"),
            );
        }

        let after_colon = path_text[colon_pos + 1..].trim();
        let mut symbols: Vec<String> = Vec::new();
        let mut alias_bindings: Vec<(String, String)> = Vec::new();
        for raw_entry in after_colon.split(',') {
            let entry = raw_entry.trim();
            if entry.is_empty() {
                continue;
            }
            if let Some((original, alias)) = split_as_rename(entry) {
                // `using M: f as g` binds only `g` to `M.f`; `f` itself stays
                // unbound (matching Julia), so it is kept out of `symbols`.
                // `Base` is sjulia's implicit global namespace, so a `Base.f`
                // rename binds to the bare global name `f` (which always resolves,
                // unlike `Base.f` as a value for many builtins).
                let source = if module_name == "Base" {
                    original
                } else {
                    format!("{module_name}.{original}")
                };
                alias_bindings.push((source, alias));
            } else {
                symbols.push(entry.to_string());
            }
        }

        if symbols.is_empty() && alias_bindings.is_empty() {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                    .with_hint("selective import must specify at least one symbol"),
            );
        }

        return Ok(UsingImport {
            module: module_name,
            symbols: Some(symbols),
            is_relative,
            relative_level,
            alias_bindings,
            span,
        });
    }

    // Regular import: `Module`, `.Module`, `Base.Sort`, or a renaming
    // `Module as Alias`. Take the path text as the module name; whitespace
    // (other than around `as`) cannot occur inside a single non-selective path.
    let raw_module_text = path_text.trim();
    if raw_module_text.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                .with_hint("using statement must specify a module name"),
        );
    }

    // Whole-module rename: `import Module as Alias` binds `Alias` to `Module`.
    let (module_text, alias_bindings) = match split_as_rename(raw_module_text) {
        Some((module_part, alias)) => {
            let source = module_part.trim_start_matches('.').to_string();
            (module_part, vec![(source, alias)])
        }
        None => (raw_module_text.to_string(), Vec::new()),
    };

    let relative_level = module_text.chars().take_while(|c| *c == '.').count();
    let is_relative = relative_level > 0;
    let module_name = module_text.trim_start_matches('.').to_string();

    if is_import_statement && alias_bindings.is_empty() {
        if let Some((parent_module, symbol)) = module_name.rsplit_once('.') {
            return Ok(UsingImport {
                module: parent_module.to_string(),
                symbols: Some(vec![symbol.to_string()]),
                is_relative,
                relative_level,
                alias_bindings,
                span,
            });
        }
    }

    Ok(UsingImport {
        module: module_name,
        symbols: None,
        is_relative,
        relative_level,
        alias_bindings,
        span,
    })
}

/// Build an expression for a dotted path like `Mz.q` or `Base.Sort.sort`.
/// The first segment becomes a variable reference and each subsequent segment a
/// field access, so `Module.symbol` resolves through the same module-qualified
/// path as if it had been written literally (Issue #8117).
fn dotted_path_expr(path: &str, span: Span) -> Expr {
    let mut segments = path.split('.').filter(|s| !s.is_empty());
    let first = segments.next().unwrap_or(path);
    let mut expr = Expr::Var(first.to_string(), span);
    for field in segments {
        expr = Expr::FieldAccess {
            object: Box::new(expr),
            field: field.to_string(),
            span,
        };
    }
    expr
}

/// Realize the `... as ...` renames of a `using`/`import` statement as runtime
/// bindings: `import M as N` becomes `N = M` and `using M: f as g` becomes
/// `g = M.f`. Returning concrete statements lets the alias name resolve to the
/// imported entity (function/value or module), which a bare `UsingImport`
/// otherwise never bound (Issue #8117).
fn using_import_alias_stmts(using_import: &UsingImport) -> Vec<Stmt> {
    using_import
        .alias_bindings
        .iter()
        .map(|(source_path, target)| Stmt::Assign {
            var: target.clone(),
            value: dotted_path_expr(source_path, using_import.span),
            span: using_import.span,
        })
        .collect()
}

/// Parse a type annotation for a macro parameter.
/// Recognizes types like Symbol, Expr, Integer, Float, String, LineNumberNode.
fn parse_macro_param_type<'a>(walker: &CstWalker<'a>, type_node: &Node<'a>) -> MacroParamType {
    let type_name = walker.text(type_node);
    match type_name {
        "Symbol" => MacroParamType::Symbol,
        "Expr" => MacroParamType::Expr,
        "Integer" | "Int" | "Int64" => MacroParamType::Integer,
        "Float" | "Float64" => MacroParamType::Float,
        "String" => MacroParamType::String,
        "LineNumberNode" => MacroParamType::LineNumberNode,
        _ => MacroParamType::Any, // Unknown types match anything
    }
}

/// Lower a macro definition: `macro name(args) body end`
/// Macros receive AST nodes as parameters, not values.
/// Returns both the MacroDef and the extracted parameter types for dispatch.
fn lower_macro_definition<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<(MacroDef, Vec<MacroParamType>)> {
    let span = walker.span(&node);
    let mut name: Option<String> = None;
    let mut params: Vec<String> = Vec::new();
    let mut param_types: Vec<MacroParamType> = Vec::new();
    let mut has_varargs = false;
    let mut body: Option<Block> = None;

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier if name.is_none() => {
                name = Some(walker.text(&child).to_string());
            }
            NodeKind::ParameterList => {
                // Extract parameter names and types from the parameter list
                let param_nodes = walker.named_children(&child);
                for (idx, param_node) in param_nodes.iter().enumerate() {
                    // Check if this is the last parameter and if it's a splat parameter
                    let is_last = idx == param_nodes.len() - 1;
                    let kind = walker.kind(param_node);
                    let text = walker.text(param_node);

                    match kind {
                        NodeKind::Identifier => {
                            params.push(text.to_string());
                            param_types.push(MacroParamType::Any);
                        }
                        NodeKind::TypedParameter => {
                            // For typed params like `ex::Expr`, extract name and type
                            let children = walker.named_children(param_node);
                            if let Some(id) = children.first() {
                                if walker.kind(id) == NodeKind::Identifier {
                                    params.push(walker.text(id).to_string());
                                }
                            }
                            // Extract type from the second child (type annotation)
                            let param_type = if children.len() > 1 {
                                parse_macro_param_type(walker, &children[1])
                            } else {
                                MacroParamType::Any
                            };
                            param_types.push(param_type);
                        }
                        NodeKind::Parameter => {
                            // The Rust parser may return Parameter with text like "x::Symbol"
                            // Check if this is a typed parameter by looking for "::" in text
                            if let Some(colon_pos) = text.find("::") {
                                let name = text[..colon_pos].trim();
                                let type_name = text[colon_pos + 2..].trim();
                                params.push(name.to_string());
                                let param_type = match type_name {
                                    "Symbol" => MacroParamType::Symbol,
                                    "Expr" => MacroParamType::Expr,
                                    "Integer" | "Int" | "Int64" => MacroParamType::Integer,
                                    "Float" | "Float64" => MacroParamType::Float,
                                    "String" => MacroParamType::String,
                                    "LineNumberNode" => MacroParamType::LineNumberNode,
                                    _ => MacroParamType::Any,
                                };
                                param_types.push(param_type);
                            } else {
                                // No type annotation, treat as identifier
                                params.push(text.to_string());
                                param_types.push(MacroParamType::Any);
                            }
                        }
                        NodeKind::SplatParameter | NodeKind::SplatExpression => {
                            // Varargs parameter: p... or p::T...
                            // Handle both SplatParameter (full-form) and SplatExpression (short-form)
                            // per Issue #2253 duality requirement
                            // Extract the parameter name from the first child (Identifier)
                            let named = walker.named_children(param_node);
                            if let Some(name_node) = named.first() {
                                if walker.kind(name_node) == NodeKind::Identifier {
                                    params.push(walker.text(name_node).to_string());
                                } else {
                                    // Try to get text as parameter name
                                    let text = walker.text(name_node);
                                    if !text.is_empty() {
                                        params.push(text.to_string());
                                    }
                                }
                            }
                            param_types.push(MacroParamType::Any);
                            // Mark that this macro has varargs (must be the last parameter)
                            if is_last {
                                has_varargs = true;
                            }
                        }
                        _ => {
                            // Try to get text as parameter name
                            if !text.is_empty() {
                                params.push(text.to_string());
                                param_types.push(MacroParamType::Any);
                            }
                        }
                    }
                }
            }
            NodeKind::Block => {
                body = Some(match lambda_ctx {
                    Some(ctx) if contains_macro_call(walker, child) => {
                        stmt::lower_block_with_ctx(walker, child, ctx)?
                    }
                    _ => stmt::lower_block(walker, child)?,
                });
            }
            _ => {}
        }
    }

    let name = name.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroDefinition, span)
            .with_hint("macro definition must have a name")
    })?;

    let body = body.unwrap_or_else(|| Block {
        stmts: vec![],
        span,
    });

    Ok((
        MacroDef {
            name,
            params,
            has_varargs,
            body,
            span,
        },
        param_types,
    ))
}

/// Lowering with include support.
/// This struct extends the basic Lowering with the ability to process include() calls.
pub struct LoweringWithInclude<'a> {
    _source: &'a str,
    walker: CstWalker<'a>,
    include_ctx: IncludeContext,
    current_file: Option<PathBuf>,
    /// Store the parsed source so it lives long enough for Node references
    parsed_rust: Option<crate::parser::RustParsedSource>,
}

impl<'a> LoweringWithInclude<'a> {
    pub fn new(source: &'a str, include_ctx: IncludeContext) -> Self {
        Self::new_with_file(source, include_ctx, None)
    }

    pub fn new_with_file(
        source: &'a str,
        include_ctx: IncludeContext,
        current_file: Option<PathBuf>,
    ) -> Self {
        Self {
            _source: source,
            walker: CstWalker::new(source),
            include_ctx,
            current_file,
            parsed_rust: None,
        }
    }

    /// Create a new lowering context with optional base directory.
    pub fn with_base_dir(source: &'a str, base_dir: Option<PathBuf>) -> Self {
        let current_file = base_dir.as_ref().map(|dir| dir.join("__source__.jl"));
        Self::new_with_file(source, IncludeContext::new(base_dir), current_file)
    }

    pub fn lower(&mut self, parse_outcome: ParseOutcome) -> LowerResult<Program> {
        let lambda_ctx = LambdaContext::with_file(self.current_file_literal());
        self.lower_with_lambda_context(parse_outcome, &lambda_ctx)
    }

    pub fn lower_with_lambda_context(
        &mut self,
        parse_outcome: ParseOutcome,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Program> {
        let ParseOutcome::Rust(parsed) = parse_outcome;
        self.parsed_rust = Some(parsed);
        // SAFETY: We know parsed is Some because we just set it, and it will live
        // as long as self.
        let parsed_ref = self.parsed_rust.as_ref().unwrap();
        let root = Node::new(parsed_ref.root(), parsed_ref.source());
        lambda_ctx.with_current_file(self.current_file_literal(), || {
            self.lower_source_file(root, lambda_ctx)
        })
    }

    /// Get a reference to the include context.
    pub fn include_context(&self) -> &IncludeContext {
        &self.include_ctx
    }

    fn current_file_literal(&self) -> Option<String> {
        self.current_file
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
    }

    /// Lower a source file (or module body) to a `Program`. Wraps the inner
    /// lowering with a type-alias scope (Issue #5055): user-defined aliases are
    /// pre-registered for this pass and the prior alias table is restored on
    /// exit, so a nested pass (e.g. the stdlib load triggered by `using Test`)
    /// cannot destroy the enclosing program's aliases.
    fn lower_source_file(
        &self,
        node: Node<'a>,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Program> {
        let scope = type_alias::snapshot();
        prescan_and_register_type_aliases(&self.walker, node);
        let result = self.lower_source_file_inner(node, lambda_ctx);
        scope.restore();
        result
    }

    fn lower_source_file_inner(
        &self,
        node: Node<'a>,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Program> {
        let mut abstract_types = Vec::new();
        let mut primitive_types = Vec::new();
        let mut type_aliases = Vec::new();
        let mut structs = Vec::new();
        let mut functions = Vec::new();
        let mut modules = Vec::new();
        let mut usings = Vec::new();
        let mut macros = Vec::new();
        let mut main_stmts = Vec::new();

        // Includes share the caller's context so definitions from one included
        // file are visible while lowering later includes in the same scope.
        let lifted_start = lambda_ctx.lifted_function_count();

        for child in self.walker.named_children(&node) {
            match self.walker.kind(&child) {
                // Skip comments
                NodeKind::LineComment | NodeKind::BlockComment => continue,
                NodeKind::AbstractDefinition => {
                    let abstract_def = abstract_::lower_abstract_definition(&self.walker, child)?;
                    lambda_ctx.add_compile_time_abstract_types(std::slice::from_ref(&abstract_def));
                    abstract_types.push(abstract_def);
                }
                NodeKind::PrimitiveDefinition => {
                    let primitive_def = primitive::lower_primitive_definition(&self.walker, child)?;
                    lambda_ctx
                        .add_compile_time_primitive_types(std::slice::from_ref(&primitive_def));
                    primitive_types.push(primitive_def);
                }
                NodeKind::StructDefinition | NodeKind::MutableStructDefinition => {
                    let struct_def = struct_::lower_struct_definition(&self.walker, child)?;
                    lambda_ctx.add_compile_time_structs(std::slice::from_ref(&struct_def));
                    structs.push(struct_def);
                }
                NodeKind::FunctionDefinition => {
                    let funcs = lower_function_all_with_macro_ctx_if_needed(
                        &self.walker,
                        child,
                        lambda_ctx,
                    )?;
                    lambda_ctx.add_compile_time_functions(&funcs);
                    functions.extend(funcs);
                }
                NodeKind::ShortFunctionDefinition => {
                    // Operator method definitions: *(x, y) = expr
                    let func = lower_operator_method_with_macro_ctx_if_needed(
                        &self.walker,
                        child,
                        lambda_ctx,
                    )?;
                    lambda_ctx.add_compile_time_functions(std::slice::from_ref(&func));
                    functions.push(func);
                }
                NodeKind::MacroDefinition => {
                    let lifted_start = lambda_ctx.lifted_function_count();
                    let (macro_def, param_types) =
                        lower_macro_definition(&self.walker, child, Some(lambda_ctx))?;
                    let macro_lambdas = lambda_ctx.lifted_functions_from_index(lifted_start);
                    lambda_ctx.add_compile_time_functions(&macro_lambdas);
                    // Register macro in context for expansion during lowering
                    lambda_ctx.add_macro(
                        &macro_def.name,
                        StoredMacroDef {
                            params: macro_def.params.clone(),
                            param_types,
                            has_varargs: macro_def.has_varargs,
                            body: macro_def.body.clone(),
                            expansion_functions: vec![],
                            expansion_structs: vec![],
                            hygiene: None,
                            span: macro_def.span,
                        },
                    );
                    macros.push(macro_def);
                }
                NodeKind::ModuleDefinition => {
                    let module = lower_module_definition(
                        &self.walker,
                        child,
                        false,
                        Some(&self.include_ctx),
                        Some(lambda_ctx),
                    )?;
                    modules.push(module);
                }
                NodeKind::BaremoduleDefinition => {
                    let module = lower_module_definition(
                        &self.walker,
                        child,
                        true,
                        Some(&self.include_ctx),
                        Some(lambda_ctx),
                    )?;
                    modules.push(module);
                }
                NodeKind::UsingStatement | NodeKind::ImportStatement => {
                    // using Module or import Module (possibly comma-separated, e.g.
                    // `using A, B` → one UsingImport per module).
                    for using_import in lower_using_statement(&self.walker, child)? {
                        // Record in lambda context for macro availability checks
                        lambda_ctx.add_using(&using_import.module);
                        // Load stdlib module macros early so they can be expanded
                        ensure_stdlib_macros_loaded(&using_import.module);
                        // Same for embedded bundled packages (e.g. Plots' @animate/@gif).
                        ensure_bundled_package_macros_loaded(&using_import.module);
                        // Bind any `... as ...` renames at the point of import so the
                        // alias name resolves to the imported entity (Issue #8117).
                        main_stmts.extend(using_import_alias_stmts(&using_import));
                        usings.push(using_import);
                    }
                }
                NodeKind::Assignment
                    if function::is_short_function_definition(&self.walker, child) =>
                {
                    // Short function definition: f(x) = expr
                    let funcs = lower_short_function_all_with_macro_ctx_if_needed(
                        &self.walker,
                        child,
                        lambda_ctx,
                    )?;
                    lambda_ctx.add_compile_time_functions(&funcs);
                    functions.extend(funcs);
                }
                NodeKind::Assignment if function::is_lambda_assignment(&self.walker, child) => {
                    // Lambda assignment: f = x -> expr
                    // May return multiple methods: the main lambda plus reduced-arity
                    // default-arg stubs for `(x, d=2) -> ...` (Issue #8047).
                    let funcs = function::lower_lambda_assignment(&self.walker, child)?;
                    lambda_ctx.add_compile_time_functions(&funcs);
                    functions.extend(funcs);
                }
                NodeKind::Assignment
                    if stmt::try_extract_type_alias_from_assignment(&self.walker, child)
                        .is_some() =>
                {
                    // Issue #5055: a plain (non-`const`) type-alias definition
                    // such as `MyVec{T} = Vector{T}` or `IntVec = Vector{Int}`.
                    // Already registered by the pre-scan; collect it and emit no
                    // runtime statement (the binding is purely a type alias).
                    if let Some(type_alias) =
                        stmt::try_extract_type_alias_from_assignment(&self.walker, child)
                    {
                        type_aliases.push(type_alias);
                    }
                }
                NodeKind::MacroCall if is_kwdef_macro(&self.walker, child) => {
                    // @kwdef struct ... end - expand to struct def + constructor
                    let (struct_def, ctor_func) = expand_kwdef_macro(&self.walker, child)?;
                    lambda_ctx.add_compile_time_structs(std::slice::from_ref(&struct_def));
                    structs.push(struct_def);
                    functions.push(ctor_func);
                }
                NodeKind::ConstStatement => {
                    // Check if this is a type alias definition
                    if let Some(type_alias) = stmt::try_extract_type_alias(&self.walker, child) {
                        type_aliases.push(type_alias);
                    }
                    // Always lower const statements so the variable is accessible at runtime
                    let stmt = stmt::lower_stmt_with_ctx(&self.walker, child, lambda_ctx)?;
                    main_stmts.push(stmt);
                }
                NodeKind::CallExpression => {
                    // Check if this is an include() call
                    if let Some(included) = self.try_process_include_call(child, lambda_ctx)? {
                        lambda_ctx
                            .add_compile_time_abstract_types(&included.program.abstract_types);
                        lambda_ctx
                            .add_compile_time_primitive_types(&included.program.primitive_types);
                        lambda_ctx.add_compile_time_structs(&included.program.structs);
                        lambda_ctx.add_compile_time_functions(&included.program.functions);
                        for macro_def in &included.program.macros {
                            lambda_ctx.add_macro_def(macro_def);
                        }
                        // Merge included content
                        let inline_block = included.merge_into(
                            &mut functions,
                            &mut structs,
                            &mut abstract_types,
                            &mut primitive_types,
                            &mut type_aliases,
                            &mut modules,
                            &mut usings,
                            &mut macros,
                            None,
                        );
                        drain_macro_expanded_structs(lambda_ctx, &mut structs);
                        // Add the inline statements from the included file
                        main_stmts.extend(inline_block.stmts);
                    } else {
                        // Not an include call, process as normal statement
                        let stmt = stmt::lower_stmt_with_ctx(&self.walker, child, lambda_ctx)?;
                        main_stmts.push(stmt);
                    }
                }
                _ => {
                    // Use context-aware lowering to handle inline lambdas
                    let stmt = stmt::lower_stmt_with_ctx(&self.walker, child, lambda_ctx)?;
                    match extract_top_level_function_defs(stmt) {
                        Ok(funcs) => functions.extend(funcs),
                        Err(stmt) => main_stmts.push(*stmt),
                    }
                }
            }
        }

        // Collect lifted lambda functions
        let lifted_functions = lambda_ctx.take_lifted_functions_from(lifted_start);
        functions.extend(lifted_functions);

        // Box scalar locals that are captured by a closure and reassigned, so the
        // closure observes the new value (Julia cell semantics, Issue #6262).
        closure_box::box_captured_reassigned_locals(&mut functions, &mut main_stmts);

        let span = self.walker.span(&node);
        Ok(Program {
            abstract_types,
            primitive_types,
            type_aliases,
            structs,
            functions,
            base_function_count: 0,
            modules,
            usings,
            macros,
            enums: vec![],
            main: Block {
                stmts: main_stmts,
                span,
            },
        })
    }

    /// Try to process a CallExpression as an include() call.
    /// Returns Some(IncludedContent) if it was an include call, None otherwise.
    fn try_process_include_call(
        &self,
        node: Node<'a>,
        lambda_ctx: &LambdaContext,
    ) -> LowerResult<Option<IncludedContent>> {
        // Check if this is a call to "include"
        let call_node = node;
        let children = self.walker.named_children(&call_node);

        // Get the function name
        let callee = children.first().ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedCallTarget,
                self.walker.span(&call_node),
            )
        })?;

        let func_name = match self.walker.kind(callee) {
            NodeKind::Identifier => self.walker.text(callee),
            _ => return Ok(None), // Not a simple identifier call
        };

        if func_name != "include" {
            return Ok(None);
        }

        let span = self.walker.span(&call_node);

        // Find the argument list
        let args_node = children
            .iter()
            .find(|n| self.walker.kind(n) == NodeKind::ArgumentList);

        // Extract the path argument
        let path = if let Some(args) = args_node {
            let arg_children = self.walker.named_children(args);
            if let Some(first_arg) = arg_children.first() {
                if self.walker.kind(first_arg) == NodeKind::StringLiteral {
                    let text = self.walker.text(first_arg);
                    text.trim_matches('"').to_string()
                } else {
                    // Dynamic path not supported
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::IncludeCall("<dynamic path>".to_string()),
                        span,
                    )
                    .with_hint("include() requires a string literal path"));
                }
            } else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::IncludeCall("<missing argument>".to_string()),
                    span,
                )
                .with_hint("include() requires a path argument"));
            }
        } else {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::IncludeCall("<no arguments>".to_string()),
                span,
            )
            .with_hint("include() requires a path argument"));
        };

        // Process the include
        let included = self
            .include_ctx
            .include_file_with_macro_context(&path, span, Some(lambda_ctx))
            .map_err(|e| {
                UnsupportedFeature::new(UnsupportedFeatureKind::Other(e.to_string()), span)
            })?;

        Ok(Some(included))
    }
}

// ==================== @kwdef Macro Expansion ====================

/// Check if a macro call is @kwdef
fn is_kwdef_macro<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> bool {
    if let Some(macro_ident) = walker.find_child(&node, NodeKind::MacroIdentifier) {
        let text = walker.text(&macro_ident);
        let name = text.trim_start_matches('@');
        return name == "kwdef";
    }
    false
}

/// Expand @kwdef macro to a struct definition and constructor function
fn expand_kwdef_macro<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(StructDef, Function)> {
    use crate::ir::core::{Expr, KwParam, Literal, Stmt};
    use crate::types::JuliaType;

    let span = walker.span(&node);

    // Find the struct definition child
    let struct_node = walker
        .named_children(&node)
        .into_iter()
        .find(|n| {
            matches!(
                walker.kind(n),
                NodeKind::StructDefinition | NodeKind::MutableStructDefinition
            )
        })
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@kwdef requires a struct definition")
        })?;

    // Parse the struct definition
    let struct_def = struct_::lower_struct_definition(walker, struct_node)?;

    // Parse default values from the struct body
    let defaults = parse_kwdef_defaults(walker, struct_node)?;

    // Build keyword parameters from struct fields with defaults
    let kwparams: Vec<KwParam> = struct_def
        .fields
        .iter()
        .map(|f| {
            // Convert TypeExpr to JuliaType if available
            let type_annotation = f.type_expr.as_ref().and_then(|te| {
                match te {
                    crate::types::TypeExpr::Concrete(jt) => Some(jt.clone()),
                    crate::types::TypeExpr::TypeVar(name) => JuliaType::from_name(name),
                    crate::types::TypeExpr::Parameterized { base, .. } => {
                        JuliaType::from_name(base)
                    }
                    crate::types::TypeExpr::RuntimeExpr(_) => None, // Runtime expressions can't be resolved at lowering
                }
            });

            // Use the parsed default value, or Undef to mark as required
            let default_expr = defaults
                .get(&f.name)
                .cloned()
                .unwrap_or(Expr::Literal(Literal::Undef, f.span));

            KwParam {
                name: f.name.clone(),
                default: default_expr,
                type_annotation,
                is_varargs: false,
                body_evaluated_default: false,
                span: f.span,
            }
        })
        .collect();

    // Create constructor body: Point(; x, y) = Point(x, y)
    let field_args: Vec<Expr> = struct_def
        .fields
        .iter()
        .map(|f| Expr::Var(f.name.clone(), f.span))
        .collect();

    let constructor_call = Expr::Call {
        function: struct_def.name.clone(),
        args: field_args,
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    };

    let body = Block {
        stmts: vec![Stmt::Expr {
            expr: constructor_call,
            span,
        }],
        span,
    };

    let constructor_func = Function {
        name: struct_def.name.clone(),
        params: vec![],
        kwparams,
        type_params: struct_def.type_params.clone(),
        return_type: None,
        body,
        is_base_extension: false,
        is_runtime_eval: false,
        span,
    };

    Ok((struct_def, constructor_func))
}

/// Parse default values from @kwdef struct body
/// Returns a map from field name to default expression
fn parse_kwdef_defaults<'a>(
    walker: &CstWalker<'a>,
    struct_node: Node<'a>,
) -> LowerResult<std::collections::HashMap<String, crate::ir::core::Expr>> {
    use std::collections::HashMap;

    let mut defaults = HashMap::new();
    let lambda_ctx = LambdaContext::new();

    // Find the block/body inside the struct
    for child in walker.named_children(&struct_node) {
        let kind = walker.kind(&child);
        if kind == NodeKind::Block || kind == NodeKind::CompoundStatement {
            // Parse each field in the block
            for field_node in walker.named_children(&child) {
                let field_kind = walker.kind(&field_node);
                if field_kind == NodeKind::Assignment {
                    // This is a field with a default value: x::Type = default
                    let children = walker.named_children(&field_node);
                    if children.len() >= 2 {
                        let lhs = children[0];
                        let rhs = children[children.len() - 1];

                        // Get the field name from the LHS
                        let field_name = match walker.kind(&lhs) {
                            NodeKind::TypedExpression => {
                                // x::Type = default
                                let typed_children = walker.named_children(&lhs);
                                if !typed_children.is_empty() {
                                    walker.text(&typed_children[0]).to_string()
                                } else {
                                    continue;
                                }
                            }
                            NodeKind::Identifier => {
                                // x = default (no type annotation)
                                walker.text(&lhs).to_string()
                            }
                            _ => continue,
                        };

                        // Parse the default value expression
                        if let Ok(default_expr) =
                            expr::lower_expr_with_ctx(walker, rhs, &lambda_ctx)
                        {
                            defaults.insert(field_name, default_expr);
                        }
                    }
                }
            }
        }
    }

    Ok(defaults)
}

/// Lower a Julia expression from text.
/// This is used to compile runtime type expressions like `Symbol(s)` in `MIME{Symbol(s)}`.
///
/// This is a simplified parser that handles common patterns:
/// - `Symbol(s)` -> Builtin { name: SymbolNew, args: [Var("s")] }
/// - Variable references like `T` -> Var("T")
pub fn lower_expr_from_text(text: &str) -> LowerResult<crate::ir::core::Expr> {
    use crate::ir::core::Expr;

    let text = text.trim();
    let span = Span::new(0, text.len(), 1, 1, 1, text.len() + 1);

    // Route the source text through the real parser + expression lowering so
    // nested constructs survive — a runtime type argument such as
    // `typeof(float(x))` (Issue #7240) contains a NESTED call inside the
    // argument list, which the previous hand-rolled comma-splitter captured as
    // a single `Var("float(x)")`. Reusing the full pipeline keeps these in sync
    // with ordinary expression lowering (nested calls, broadcasts, operators,
    // string literals, …) instead of re-implementing a partial parser here.
    if let Some(expr) = try_lower_expr_via_parser(text) {
        return Ok(expr);
    }

    // Fallback: treat as variable reference. Reached only when the text fails to
    // parse as a standalone expression (e.g. a bare type variable like `T`,
    // which the parser yields as an identifier and we already handle).
    Ok(Expr::Var(text.to_string(), span))
}

/// Parse `text` as a single Julia expression and lower it to Core IR using the
/// real parser/lowering pipeline. Returns `None` when the text does not parse
/// into exactly one expression-bearing top-level node, leaving the caller to
/// fall back. Used by `lower_expr_from_text` for runtime type-argument
/// expressions captured as source strings (Issue #7240).
fn try_lower_expr_via_parser(text: &str) -> Option<crate::ir::core::Expr> {
    let mut parser = Parser::new().ok()?;
    let parse_outcome = parser.parse(text).ok()?;
    let ParseOutcome::Rust(parsed) = parse_outcome;
    let root = Node::new(parsed.root(), parsed.source());
    let walker = CstWalker::new(text);

    // The first non-comment named child of the source file is the expression.
    let first = walker.named_children(&root).into_iter().find(|child| {
        !matches!(
            walker.kind(child),
            NodeKind::LineComment | NodeKind::BlockComment
        )
    })?;

    expr::lower_expr(&walker, first).ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn conditional_export_collection_preserves_source_order_7959() {
        let program = crate::pipeline::parse_and_lower(
            r#"
            module SequentialExport7959
            if :x in names(SequentialExport7959)
                export y
            end
            export x
            x = 1
            y = 2
            end

            module PriorExport7959
            export x
            if :x in names(PriorExport7959)
                export y
            end
            x = 1
            y = 2
            end
            "#,
        )
        .expect("source should lower");

        let sequential = program
            .modules
            .iter()
            .find(|module| module.name == "SequentialExport7959")
            .expect("sequential module");
        assert!(sequential.exports.contains(&"x".to_string()));
        assert!(!sequential.exports.contains(&"y".to_string()));

        let prior = program
            .modules
            .iter()
            .find(|module| module.name == "PriorExport7959")
            .expect("prior module");
        assert!(prior.exports.contains(&"x".to_string()));
        assert!(prior.exports.contains(&"y".to_string()));
    }
}
