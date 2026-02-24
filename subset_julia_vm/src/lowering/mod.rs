pub mod abstract_;
pub mod expr;
pub mod function;
mod macros_registry;
pub mod stmt;
pub mod struct_;

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use std::collections::HashMap;

use crate::error::{IncludeError, UnsupportedFeature, UnsupportedFeatureKind};
use crate::include::{read_include_file, resolve_include_path};
use crate::ir::core::{
    AbstractTypeDef, Block, Function, MacroDef, Module, Program, StructDef, UsingImport,
};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::parser::{ParseOutcome, Parser};
use crate::span::Span;
use crate::stdlib_loader::ensure_stdlib_macros_loaded;
use macros_registry::check_type_compatibility;

pub use macros_registry::{get_node_macro_type, MacroParamType, StoredMacroDef};

pub type LowerResult<T> = Result<T, UnsupportedFeature>;

/// Result type for include operations that can fail with IncludeError.
pub type IncludeResult<T> = Result<T, IncludeError>;

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
    /// Current file path (for @__FILE__ and @__DIR__ macros)
    /// None means REPL or unknown source
    current_file: Option<String>,
}

impl LambdaContext {
    pub fn new() -> Self {
        Self {
            lifted_functions: RefCell::new(Vec::new()),
            lambda_counter: RefCell::new(0),
            usings: RefCell::new(HashSet::new()),
            macros: RefCell::new(HashMap::new()),
            current_file: None,
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
            current_file: file_path,
        }
    }

    /// Get the current file path (for @__FILE__ macro).
    /// Returns "none" for REPL/unknown sources.
    pub fn get_current_file(&self) -> String {
        self.current_file
            .clone()
            .unwrap_or_else(|| "none".to_string())
    }

    /// Get the current directory (for @__DIR__ macro).
    /// Returns "." for REPL/unknown sources.
    pub fn get_current_dir(&self) -> String {
        match &self.current_file {
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

    /// Record that a module has been imported via `using`.
    pub fn add_using(&self, module: &str) {
        self.usings.borrow_mut().insert(module.to_string());
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

        // 7. Create child context with new base directory
        let child_base = resolved.parent().map(|p| p.to_path_buf());
        let child_ctx = self.child(child_base);

        // 8. Lower the parsed content
        let mut lowering = LoweringWithInclude::new(&content, child_ctx);
        let program = lowering
            .lower(parse_outcome)
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
    /// The resolved file path.
    pub file_path: PathBuf,
    /// The span of the include call in the parent file.
    pub span: Span,
}

impl IncludedContent {
    /// Merge the included content into a parent program.
    /// Functions, structs, abstract types, modules, and usings are added to the parent.
    /// Main statements are returned to be inlined at the include site.
    pub fn merge_into(
        self,
        functions: &mut Vec<Function>,
        structs: &mut Vec<StructDef>,
        abstract_types: &mut Vec<AbstractTypeDef>,
        modules: &mut Vec<Module>,
        usings: &mut Vec<UsingImport>,
    ) -> Block {
        functions.extend(self.program.functions);
        structs.extend(self.program.structs);
        abstract_types.extend(self.program.abstract_types);
        modules.extend(self.program.modules);
        usings.extend(self.program.usings);
        self.program.main
    }
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

    fn lower_source_file(&self, node: Node<'a>) -> LowerResult<Program> {
        let mut abstract_types = Vec::new();
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
        }

        for child in self.walker.named_children(&node) {
            match self.walker.kind(&child) {
                // Skip comments
                NodeKind::LineComment | NodeKind::BlockComment => continue,
                NodeKind::AbstractDefinition => {
                    let abstract_def = abstract_::lower_abstract_definition(&self.walker, child)?;
                    abstract_types.push(abstract_def);
                }
                NodeKind::StructDefinition | NodeKind::MutableStructDefinition => {
                    let struct_def = struct_::lower_struct_definition(&self.walker, child)?;
                    structs.push(struct_def);
                }
                NodeKind::FunctionDefinition => {
                    let funcs = function::lower_function_all(&self.walker, child)?;
                    functions.extend(funcs);
                }
                NodeKind::ShortFunctionDefinition => {
                    // Operator method definitions: *(x, y) = expr
                    let func = function::lower_operator_method(&self.walker, child)?;
                    functions.push(func);
                }
                NodeKind::MacroDefinition => {
                    let (macro_def, param_types) = lower_macro_definition(&self.walker, child)?;
                    // Register macro in context for expansion during lowering
                    lambda_ctx.add_macro(
                        &macro_def.name,
                        StoredMacroDef {
                            params: macro_def.params.clone(),
                            param_types,
                            has_varargs: macro_def.has_varargs,
                            body: macro_def.body.clone(),
                            span: macro_def.span,
                        },
                    );
                    macros.push(macro_def);
                }
                NodeKind::ModuleDefinition => {
                    let module = lower_module_definition(&self.walker, child, false)?;
                    modules.push(module);
                }
                NodeKind::BaremoduleDefinition => {
                    let module = lower_module_definition(&self.walker, child, true)?;
                    modules.push(module);
                }
                NodeKind::UsingStatement | NodeKind::ImportStatement => {
                    // using Module or import Module
                    let using_import = lower_using_statement(&self.walker, child)?;
                    // Record in lambda context for macro availability checks
                    lambda_ctx.add_using(&using_import.module);
                    // Load stdlib module macros early so they can be expanded
                    ensure_stdlib_macros_loaded(&using_import.module);
                    usings.push(using_import);
                }
                NodeKind::Assignment
                    if function::is_short_function_definition(&self.walker, child) =>
                {
                    // Short function definition: f(x) = expr
                    let funcs = function::lower_short_function_all(&self.walker, child)?;
                    functions.extend(funcs);
                }
                NodeKind::Assignment if function::is_lambda_assignment(&self.walker, child) => {
                    // Lambda assignment: f = x -> expr
                    let func = function::lower_lambda_assignment(&self.walker, child)?;
                    functions.push(func);
                }
                NodeKind::MacroCall if is_kwdef_macro(&self.walker, child) => {
                    // @kwdef struct ... end - expand to struct def + constructor
                    let (struct_def, ctor_func) = expand_kwdef_macro(&self.walker, child)?;
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
                    main_stmts.push(stmt);
                }
                _ => {
                    // Use context-aware lowering to handle inline lambdas
                    let stmt = stmt::lower_stmt_with_ctx(&self.walker, child, &lambda_ctx)?;
                    main_stmts.push(stmt);
                }
            }
        }

        // Collect lifted lambda functions
        let lifted_functions = lambda_ctx.take_lifted_functions();
        functions.extend(lifted_functions);

        let span = self.walker.span(&node);
        Ok(Program {
            abstract_types,
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

/// Lower a module definition: `module Name ... end` or `baremodule Name ... end`
fn lower_module_definition<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    is_bare: bool,
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
    let mut type_aliases = Vec::new();
    let mut submodules = Vec::new();
    let mut usings = Vec::new();
    let mut macros = Vec::new();
    let mut exports = Vec::new();
    let mut publics = Vec::new();
    let mut body_stmts = Vec::new();

    if let Some(block_node) = body_node {
        for child in walker.named_children(&block_node) {
            match walker.kind(&child) {
                // Skip comments
                NodeKind::LineComment | NodeKind::BlockComment => continue,
                // Handle struct definitions
                NodeKind::StructDefinition | NodeKind::MutableStructDefinition => {
                    let struct_def = struct_::lower_struct_definition(walker, child)?;
                    structs.push(struct_def);
                }
                // Handle abstract type definitions
                NodeKind::AbstractDefinition => {
                    let abstract_def = abstract_::lower_abstract_definition(walker, child)?;
                    abstract_types.push(abstract_def);
                }
                NodeKind::FunctionDefinition => {
                    let funcs = function::lower_function_all(walker, child)?;
                    functions.extend(funcs);
                }
                NodeKind::ShortFunctionDefinition => {
                    // Operator method definitions: *(x, y) = expr
                    let func = function::lower_operator_method(walker, child)?;
                    functions.push(func);
                }
                NodeKind::ModuleDefinition => {
                    // Nested module: recursively lower
                    let submodule = lower_module_definition(walker, child, false)?;
                    submodules.push(submodule);
                }
                NodeKind::BaremoduleDefinition => {
                    // Nested baremodule: recursively lower
                    let submodule = lower_module_definition(walker, child, true)?;
                    submodules.push(submodule);
                }
                NodeKind::UsingStatement | NodeKind::ImportStatement => {
                    // using Module or import Module
                    let using_import = lower_using_statement(walker, child)?;
                    usings.push(using_import);
                }
                NodeKind::ExportStatement => {
                    // export func1, func2, ...
                    let export_names = lower_export_statement(walker, child)?;
                    exports.extend(export_names);
                }
                NodeKind::PublicStatement => {
                    // public func1, func2, ... (Julia 1.11+)
                    let public_names = lower_public_statement(walker, child)?;
                    publics.extend(public_names);
                }
                NodeKind::MacroDefinition => {
                    // Macro definition within module
                    let (macro_def, _param_types) = lower_macro_definition(walker, child)?;
                    macros.push(macro_def);
                }
                NodeKind::Assignment if function::is_short_function_definition(walker, child) => {
                    // Short function definition: f(x) = expr
                    let funcs = function::lower_short_function_all(walker, child)?;
                    functions.extend(funcs);
                }
                NodeKind::Assignment if function::is_lambda_assignment(walker, child) => {
                    // Lambda assignment: f = x -> expr
                    let func = function::lower_lambda_assignment(walker, child)?;
                    functions.push(func);
                }
                NodeKind::ConstStatement => {
                    // Check if this is a type alias definition
                    if let Some(type_alias) = stmt::try_extract_type_alias(walker, child) {
                        type_aliases.push(type_alias);
                    }
                    // Always lower const statements so the variable is accessible at runtime
                    let stmt = stmt::lower_stmt(walker, child)?;
                    body_stmts.push(stmt);
                }
                _ => {
                    let stmt = stmt::lower_stmt(walker, child)?;
                    body_stmts.push(stmt);
                }
            }
        }
    }

    let body = Block {
        stmts: body_stmts,
        span,
    };

    Ok(Module {
        name,
        is_bare,
        functions,
        structs,
        abstract_types,
        type_aliases,
        submodules,
        usings,
        macros,
        exports,
        publics,
        body,
        span,
    })
}

/// Lower an export statement: `export func1, func2, ...`
/// Returns the list of exported names.
fn lower_export_statement<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Vec<String>> {
    let mut names = Vec::new();

    // Collect all identifier children as exported names
    for child in walker.named_children(&node) {
        if walker.kind(&child) == NodeKind::Identifier {
            names.push(walker.text(&child).to_string());
        }
    }

    Ok(names)
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
/// Returns a UsingImport with the module name and optional list of imported symbols.
fn lower_using_statement<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<UsingImport> {
    let span = walker.span(&node);

    // Get the full text to detect selective import syntax
    let full_text = walker.text(&node);

    // Check if this is a selective import: "using Module: func1, func2"
    if let Some(colon_pos) = full_text.find(':') {
        // Extract module name (before the colon)
        let before_colon = full_text[..colon_pos].trim();
        let mut module_name = before_colon
            .strip_prefix("using")
            .or_else(|| before_colon.strip_prefix("import"))
            .map(|s| s.trim())
            .unwrap_or(before_colon)
            .to_string();

        // Check for relative import (leading dot)
        let is_relative = module_name.starts_with('.');
        if is_relative {
            module_name = module_name.trim_start_matches('.').to_string();
        }

        if module_name.is_empty() {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                    .with_hint("using statement must specify a module name"),
            );
        }

        // Extract symbols (after the colon)
        let after_colon = full_text[colon_pos + 1..].trim();
        let symbols: Vec<String> = after_colon
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if symbols.is_empty() {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                    .with_hint("selective import must specify at least one symbol"),
            );
        }

        return Ok(UsingImport {
            module: module_name,
            symbols: Some(symbols),
            is_relative,
            span,
        });
    }

    // Regular import: "using Module" or "using .Module"
    let named_children = walker.named_children(&node);
    if named_children.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                .with_hint("using statement must specify a module name"),
        );
    }

    let first_child = named_children[0];
    let raw_module_text = match walker.kind(&first_child) {
        NodeKind::Identifier => walker.text(&first_child).to_string(),
        _ => {
            // For other node types (like import_path for `.Module`), extract the text
            // This handles cases like scoped identifiers (Base.Sort) or relative imports (.Module)
            let text = walker.text(&first_child);
            if text.is_empty() {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::UsingStatement, span)
                        .with_hint("using statement must specify a module name"),
                );
            }
            text.to_string()
        }
    };

    // Check for relative import (leading dot)
    let is_relative = raw_module_text.starts_with('.');
    let module_name = if is_relative {
        raw_module_text.trim_start_matches('.').to_string()
    } else {
        raw_module_text
    };

    Ok(UsingImport {
        module: module_name,
        symbols: None,
        is_relative,
        span,
    })
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
) -> LowerResult<(MacroDef, Vec<MacroParamType>)> {
    let span = walker.span(&node);
    let mut name: Option<String> = None;
    let mut params: Vec<String> = Vec::new();
    let mut param_types: Vec<MacroParamType> = Vec::new();
    let mut has_varargs = false;
    let mut body: Option<Block> = None;

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                if name.is_none() {
                    name = Some(walker.text(&child).to_string());
                }
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
                body = Some(stmt::lower_block(walker, child)?);
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
    /// Store the parsed source so it lives long enough for Node references
    parsed_rust: Option<crate::parser::RustParsedSource>,
}

impl<'a> LoweringWithInclude<'a> {
    pub fn new(source: &'a str, include_ctx: IncludeContext) -> Self {
        Self {
            _source: source,
            walker: CstWalker::new(source),
            include_ctx,
            parsed_rust: None,
        }
    }

    /// Create a new lowering context with optional base directory.
    pub fn with_base_dir(source: &'a str, base_dir: Option<PathBuf>) -> Self {
        Self::new(source, IncludeContext::new(base_dir))
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

    /// Get a reference to the include context.
    pub fn include_context(&self) -> &IncludeContext {
        &self.include_ctx
    }

    fn lower_source_file(&self, node: Node<'a>) -> LowerResult<Program> {
        let mut abstract_types = Vec::new();
        let mut type_aliases = Vec::new();
        let mut structs = Vec::new();
        let mut functions = Vec::new();
        let mut modules = Vec::new();
        let mut usings = Vec::new();
        let mut macros = Vec::new();
        let mut main_stmts = Vec::new();

        // Create lambda context for collecting lifted anonymous functions
        let lambda_ctx = LambdaContext::new();

        for child in self.walker.named_children(&node) {
            match self.walker.kind(&child) {
                // Skip comments
                NodeKind::LineComment | NodeKind::BlockComment => continue,
                NodeKind::AbstractDefinition => {
                    let abstract_def = abstract_::lower_abstract_definition(&self.walker, child)?;
                    abstract_types.push(abstract_def);
                }
                NodeKind::StructDefinition | NodeKind::MutableStructDefinition => {
                    let struct_def = struct_::lower_struct_definition(&self.walker, child)?;
                    structs.push(struct_def);
                }
                NodeKind::FunctionDefinition => {
                    let funcs = function::lower_function_all(&self.walker, child)?;
                    functions.extend(funcs);
                }
                NodeKind::ShortFunctionDefinition => {
                    // Operator method definitions: *(x, y) = expr
                    let func = function::lower_operator_method(&self.walker, child)?;
                    functions.push(func);
                }
                NodeKind::MacroDefinition => {
                    let (macro_def, param_types) = lower_macro_definition(&self.walker, child)?;
                    // Register macro in context for expansion during lowering
                    lambda_ctx.add_macro(
                        &macro_def.name,
                        StoredMacroDef {
                            params: macro_def.params.clone(),
                            param_types,
                            has_varargs: macro_def.has_varargs,
                            body: macro_def.body.clone(),
                            span: macro_def.span,
                        },
                    );
                    macros.push(macro_def);
                }
                NodeKind::ModuleDefinition => {
                    let module = lower_module_definition(&self.walker, child, false)?;
                    modules.push(module);
                }
                NodeKind::BaremoduleDefinition => {
                    let module = lower_module_definition(&self.walker, child, true)?;
                    modules.push(module);
                }
                NodeKind::UsingStatement | NodeKind::ImportStatement => {
                    // using Module or import Module
                    let using_import = lower_using_statement(&self.walker, child)?;
                    // Record in lambda context for macro availability checks
                    lambda_ctx.add_using(&using_import.module);
                    // Load stdlib module macros early so they can be expanded
                    ensure_stdlib_macros_loaded(&using_import.module);
                    usings.push(using_import);
                }
                NodeKind::Assignment
                    if function::is_short_function_definition(&self.walker, child) =>
                {
                    // Short function definition: f(x) = expr
                    let funcs = function::lower_short_function_all(&self.walker, child)?;
                    functions.extend(funcs);
                }
                NodeKind::Assignment if function::is_lambda_assignment(&self.walker, child) => {
                    // Lambda assignment: f = x -> expr
                    let func = function::lower_lambda_assignment(&self.walker, child)?;
                    functions.push(func);
                }
                NodeKind::MacroCall if is_kwdef_macro(&self.walker, child) => {
                    // @kwdef struct ... end - expand to struct def + constructor
                    let (struct_def, ctor_func) = expand_kwdef_macro(&self.walker, child)?;
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
                    main_stmts.push(stmt);
                }
                NodeKind::CallExpression => {
                    // Check if this is an include() call
                    if let Some(included) = self.try_process_include_call(child, &lambda_ctx)? {
                        // Merge included content
                        let inline_block = included.merge_into(
                            &mut functions,
                            &mut structs,
                            &mut abstract_types,
                            &mut modules,
                            &mut usings,
                        );
                        // Add the inline statements from the included file
                        main_stmts.extend(inline_block.stmts);
                    } else {
                        // Not an include call, process as normal statement
                        let stmt = stmt::lower_stmt_with_ctx(&self.walker, child, &lambda_ctx)?;
                        main_stmts.push(stmt);
                    }
                }
                _ => {
                    // Use context-aware lowering to handle inline lambdas
                    let stmt = stmt::lower_stmt_with_ctx(&self.walker, child, &lambda_ctx)?;
                    main_stmts.push(stmt);
                }
            }
        }

        // Collect lifted lambda functions
        let lifted_functions = lambda_ctx.take_lifted_functions();
        functions.extend(lifted_functions);

        let span = self.walker.span(&node);
        Ok(Program {
            abstract_types,
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
        _lambda_ctx: &LambdaContext,
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
        let included = self.include_ctx.include_file(&path, span).map_err(|e| {
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
    use crate::ir::core::{Expr, Literal};
    use expr::map_builtin_name;

    let text = text.trim();
    let span = Span::new(0, text.len(), 1, 1, 1, text.len() + 1);

    // Check if this looks like a function call: Name(args)
    if let Some(open_paren) = text.find('(') {
        if text.ends_with(')') {
            let func_name = text[..open_paren].trim();
            let args_str = &text[open_paren + 1..text.len() - 1];

            // Parse arguments (simple comma-split; nested parens not handled)
            let args: Vec<Expr> = if args_str.trim().is_empty() {
                Vec::new()
            } else {
                args_str
                    .split(',')
                    .map(|arg| {
                        let arg = arg.trim();
                        // Check for string literals
                        if arg.starts_with('"') && arg.ends_with('"') {
                            let s = arg[1..arg.len() - 1].to_string();
                            Expr::Literal(Literal::Str(s), span)
                        } else {
                            Expr::Var(arg.to_string(), span)
                        }
                    })
                    .collect()
            };

            // Check if this is a builtin function (like Symbol)
            if let Some(builtin_op) = map_builtin_name(func_name) {
                return Ok(Expr::Builtin {
                    name: builtin_op,
                    args,
                    span,
                });
            }

            return Ok(Expr::Call {
                function: func_name.to_string(),
                args,
                kwargs: Vec::new(),
                splat_mask: Vec::new(),
                kwargs_splat_mask: Vec::new(),
                span,
            });
        }
    }

    // Fallback: treat as variable reference
    Ok(Expr::Var(text.to_string(), span))
}
