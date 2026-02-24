use std::collections::HashMap;

use crate::compile::compile_with_cache_with_globals;
use crate::ir::core::{Expr, Function, Literal, Module, Program, Stmt, StructDef, UsingImport};
use crate::loader;
use crate::lowering::Lowering;
use crate::parser::Parser;
use crate::rng::StableRng;
use crate::span::Span;
use crate::vm::{FunctionValue, StructInstance, Value, ValueType, Vm};

use super::converters::{
    callable_value_to_expr, extract_assigned_variables, struct_instance_to_literal,
    value_to_literal,
};
use super::globals::{REPLGlobals, REPLResult};

/// REPL session that maintains state across evaluations.
pub struct REPLSession {
    /// Random seed for RNG
    seed: u64,
    /// Persistent global variables
    globals: REPLGlobals,
    /// Previously defined functions (accumulated across evaluations)
    functions: Vec<Function>,
    /// Fast index for function name -> index in `functions`
    function_index: HashMap<String, usize>,
    /// Previously defined structs (accumulated across evaluations)
    structs: Vec<StructDef>,
    /// Fast index for struct name -> index in `structs`
    struct_index: HashMap<String, usize>,
    /// Previously defined modules (accumulated across evaluations)
    modules: Vec<Module>,
    /// Fast index for module name -> index in `modules`
    module_index: HashMap<String, usize>,
    /// Imported modules via `using` (accumulated across evaluations)
    usings: Vec<UsingImport>,
    /// Persistent struct instances (name -> Literal::Struct)
    struct_instances: HashMap<String, Literal>,
    /// Last evaluation result (for `ans`)
    ans: Option<Value>,
    /// Evaluation counter for RNG seed variation
    eval_count: u64,
    /// Last VM's struct heap (for resolving StructRefs in display)
    last_struct_heap: Vec<StructInstance>,
    /// Global variable types (for type inference in next compilation)
    /// Maps variable name -> (struct_name, type_id) for Struct types
    /// or variable name -> ValueType for other types
    global_types: HashMap<String, ValueType>,
    /// Struct names for variables (used to resolve type_id from struct_table)
    global_struct_names: HashMap<String, String>,
}

impl REPLSession {
    /// Create a new REPL session with the given RNG seed.
    ///
    /// The session is initialized with `InteractiveUtils` automatically imported,
    /// matching Julia's standard REPL behavior where `versioninfo()` and other
    /// utilities are available by default.
    pub fn new(seed: u64) -> Self {
        // Automatically import InteractiveUtils, matching Julia's REPL behavior
        // Julia's REPL loads InteractiveUtils by default for convenience functions
        // like versioninfo(), supertypes(), etc.
        let default_usings = vec![UsingImport {
            module: "InteractiveUtils".to_string(),
            symbols: None, // Import all exported symbols
            is_relative: false,
            span: Span::new(0, 0, 0, 0, 0, 0), // Synthetic span for auto-import
        }];

        Self {
            seed,
            globals: REPLGlobals::new(),
            functions: Vec::new(),
            function_index: HashMap::new(),
            structs: Vec::new(),
            struct_index: HashMap::new(),
            modules: Vec::new(),
            module_index: HashMap::new(),
            usings: default_usings,
            struct_instances: HashMap::new(),
            ans: None,
            eval_count: 0,
            last_struct_heap: Vec::new(),
            global_types: HashMap::new(),
            global_struct_names: HashMap::new(),
        }
    }

    /// Evaluate Julia code in this session.
    /// Variables defined here will persist for future evaluations.
    pub fn eval(&mut self, input: &str) -> REPLResult {
        crate::cancel::reset();

        // Parse input
        let mut parser = match Parser::new() {
            Ok(p) => p,
            Err(e) => {
                return REPLResult::error(format!("Parser init failed: {}", e), String::new())
            }
        };

        let outcome = match parser.parse(input) {
            Ok(o) => o,
            Err(e) => return REPLResult::error(format!("Parse error: {}", e), String::new()),
        };

        // Lower to IR
        let mut lowering = Lowering::new_with_usings(input, &self.usings);
        let mut program = match lowering.lower(outcome) {
            Ok(p) => p,
            Err(e) => {
                return REPLResult::error(format!("{:?}: {:?}", e.kind, e.hint), String::new())
            }
        };

        // Merge with existing functions and structs
        self.merge_definitions(&mut program);

        let existing_modules: std::collections::HashSet<String> =
            program.modules.iter().map(|m| m.name.clone()).collect();
        // Filter out:
        // 1. Relative imports (using .Module) - these reference user-defined modules, not packages
        // 2. Modules that already exist in the program
        let usings_to_load: Vec<UsingImport> = program
            .usings
            .iter()
            .filter(|u| !u.is_relative && !existing_modules.contains(&u.module))
            .cloned()
            .collect();

        if !usings_to_load.is_empty() {
            let mut package_loader = loader::PackageLoader::new(loader::LoaderConfig::from_env());
            let loaded_modules = match package_loader.load_for_usings(&usings_to_load) {
                Ok(modules) => modules,
                Err(e) => return REPLResult::error(format!("Load error: {}", e), String::new()),
            };

            for module in loaded_modules {
                if !existing_modules.contains(&module.name) {
                    program.modules.push(module);
                }
            }
        }

        // Inject global variable initializations at the start of main
        self.inject_globals(&mut program);

        // Resolve struct type_ids from struct_names before compilation
        // This is needed because VM's type_id may not match compile-time struct_table indices
        let resolved_global_types = self.global_types.clone();
        // Note: We'll resolve struct_names to type_ids in compile_core_program_with_globals
        // after struct_table is built

        // Compile with global types from previous evaluations
        let compiled = match compile_with_cache_with_globals(
            &program,
            &resolved_global_types,
            &self.global_struct_names,
        ) {
            Ok(c) => c,
            Err(e) => return REPLResult::error(format!("Compile error: {:?}", e), String::new()),
        };

        // Run with a seed that varies per evaluation for different random sequences
        let eval_seed = self.seed.wrapping_add(self.eval_count);
        self.eval_count += 1;

        let rng = StableRng::new(eval_seed);
        let mut vm = Vm::new_program(compiled, rng);

        match vm.run() {
            Ok(value) => {
                let output = vm.get_output().to_string();

                // Extract new variable assignments from the result
                self.extract_globals_from_vm(&vm, &program);

                // Store VM's struct heap for resolving StructRefs in display
                self.last_struct_heap = vm.get_struct_heap().to_vec();

                // Check if a new function was defined in this evaluation
                // If so, return the function object instead of Nothing/previous value
                let new_functions: Vec<&Function> = program
                    .functions
                    .iter()
                    .filter(|f| {
                        !self
                            .functions
                            .iter()
                            .any(|existing| existing.name == f.name)
                    })
                    .collect();

                let return_value = if new_functions.len() == 1 {
                    // Single new function defined - return it as a Function value
                    // This matches Julia REPL behavior: "f (generic function with 1 method)"
                    Value::Function(FunctionValue {
                        name: new_functions[0].name.clone(),
                    })
                } else {
                    // No new functions, or multiple functions - use the VM's return value
                    value
                };

                // Store result in ans
                if !matches!(return_value, Value::Nothing) {
                    self.ans = Some(return_value.clone());
                    self.globals.set("ans", return_value.clone());
                }

                // Store new function and struct definitions
                self.store_definitions(&program);

                REPLResult::success(return_value, output)
            }
            Err(e) => {
                let output = vm.get_output().to_string();
                REPLResult::error(format!("{}", e), output)
            }
        }
    }

    /// Merge existing function, struct, and module definitions into the program.
    fn merge_definitions(&self, program: &mut Program) {
        // Collect new function names first
        let new_func_names: std::collections::HashSet<String> =
            program.functions.iter().map(|f| f.name.clone()).collect();

        // Add existing functions that aren't being redefined
        for func in &self.functions {
            if !new_func_names.contains(&func.name) {
                program.functions.push(func.clone());
            }
        }

        // Collect new struct names
        let new_struct_names: std::collections::HashSet<String> =
            program.structs.iter().map(|s| s.name.clone()).collect();

        // Add existing structs that aren't being redefined
        for s in &self.structs {
            if !new_struct_names.contains(&s.name) {
                program.structs.push(s.clone());
            }
        }

        // Collect new module names
        let new_module_names: std::collections::HashSet<String> =
            program.modules.iter().map(|m| m.name.clone()).collect();

        // Add existing modules that aren't being redefined
        for m in &self.modules {
            if !new_module_names.contains(&m.name) {
                program.modules.push(m.clone());
            }
        }

        // Merge usings - add existing usings that aren't already in the program
        let new_using_modules: std::collections::HashSet<String> =
            program.usings.iter().map(|u| u.module.clone()).collect();

        for using in &self.usings {
            if !new_using_modules.contains(&using.module) {
                program.usings.push(using.clone());
            }
        }
    }

    /// Inject global variable initializations at the start of the program.
    fn inject_globals(&mut self, program: &mut Program) {
        let mut init_stmts = Vec::new();
        let dummy_span = Span::new(0, 0, 0, 0, 0, 0);

        // Create assignment statements for each global variable
        for name in self.globals.variable_names() {
            if let Some(value) = self.globals.get(&name) {
                // Handle StructRef specially - convert to StructInstance and store in struct_instances
                if let Value::StructRef(idx) = value {
                    if let Some(struct_instance) = self.last_struct_heap.get(idx) {
                        // Use struct_name directly (Rational is defined in Pure Julia, not in program.structs)
                        if let Some(literal) = struct_instance_to_literal(
                            struct_instance,
                            &struct_instance.struct_name,
                        ) {
                            // Store in struct_instances for future use
                            self.struct_instances.insert(name.clone(), literal.clone());
                            let stmt = Stmt::Assign {
                                var: name,
                                value: Expr::Literal(literal, dummy_span),
                                span: dummy_span,
                            };
                            init_stmts.push(stmt);
                        }
                    }
                } else if let Value::Array(ref arr) = value {
                    // Handle Array with StructRefs - convert each StructRef to Literal::Struct
                    let arr_borrow = arr.borrow();
                    if let crate::vm::ArrayData::StructRefs(ref struct_refs) = arr_borrow.data {
                        let mut elements = Vec::new();
                        for &struct_ref_idx in struct_refs {
                            if let Some(struct_instance) = self.last_struct_heap.get(struct_ref_idx)
                            {
                                if let Some(literal) = struct_instance_to_literal(
                                    struct_instance,
                                    &struct_instance.struct_name,
                                ) {
                                    elements.push(Expr::Literal(literal, dummy_span));
                                } else {
                                    // If conversion fails, skip this array
                                    break;
                                }
                            } else {
                                // If struct_instance not found, skip this array
                                break;
                            }
                        }
                        // Only create assignment if all elements were converted successfully
                        if elements.len() == struct_refs.len() {
                            let stmt = Stmt::Assign {
                                var: name,
                                value: Expr::ArrayLiteral {
                                    elements,
                                    shape: arr_borrow.shape.clone(),
                                    span: dummy_span,
                                },
                                span: dummy_span,
                            };
                            init_stmts.push(stmt);
                        }
                    } else if let Some(literal) = value_to_literal(&value) {
                        // Handle other array types (F64, I64, Bool, etc.)
                        let stmt = Stmt::Assign {
                            var: name,
                            value: Expr::Literal(literal, dummy_span),
                            span: dummy_span,
                        };
                        init_stmts.push(stmt);
                    }
                } else if let Value::Memory(ref mem) = value {
                    // Memory → Array (Issue #2764)
                    let arr = crate::vm::util::memory_to_array_ref(mem);
                    let arr_val = Value::Array(arr);
                    if let Some(literal) = value_to_literal(&arr_val) {
                        let stmt = Stmt::Assign {
                            var: name,
                            value: Expr::Literal(literal, dummy_span),
                            span: dummy_span,
                        };
                        init_stmts.push(stmt);
                    }
                } else if let Value::NamedTuple(ref nt) = value {
                    // Handle NamedTuple - convert to NamedTupleLiteral
                    let mut fields = Vec::new();
                    let mut all_convertible = true;
                    for (field_name, field_value) in nt.names.iter().zip(nt.values.iter()) {
                        if let Some(field_literal) = value_to_literal(field_value) {
                            fields.push((
                                field_name.clone(),
                                Expr::Literal(field_literal, dummy_span),
                            ));
                        } else {
                            // If any field cannot be converted, skip this NamedTuple
                            all_convertible = false;
                            break;
                        }
                    }
                    if all_convertible {
                        let stmt = Stmt::Assign {
                            var: name,
                            value: Expr::NamedTupleLiteral {
                                fields,
                                span: dummy_span,
                            },
                            span: dummy_span,
                        };
                        init_stmts.push(stmt);
                    }
                } else if let Some(literal) = value_to_literal(&value) {
                    let stmt = Stmt::Assign {
                        var: name,
                        value: Expr::Literal(literal, dummy_span),
                        span: dummy_span,
                    };
                    init_stmts.push(stmt);
                } else if let Some(expr) = callable_value_to_expr(&value, dummy_span) {
                    // Handle Function and ComposedFunction
                    let stmt = Stmt::Assign {
                        var: name,
                        value: expr,
                        span: dummy_span,
                    };
                    init_stmts.push(stmt);
                }
            }
        }

        // Create assignment statements for struct instances
        for (name, literal) in &self.struct_instances {
            let stmt = Stmt::Assign {
                var: name.clone(),
                value: Expr::Literal(literal.clone(), dummy_span),
                span: dummy_span,
            };
            init_stmts.push(stmt);
        }

        // Prepend initialization statements to main
        if !init_stmts.is_empty() {
            init_stmts.append(&mut program.main.stmts);
            program.main.stmts = init_stmts;
        }
    }

    /// Extract global variables from VM state after execution.
    /// This is a simplified version that tracks assignments in the main block.
    fn extract_globals_from_vm<R: crate::rng::RngLike>(&mut self, vm: &Vm<R>, program: &Program) {
        // Get the top-level variable names from the program's main block
        let assigned_vars = extract_assigned_variables(&program.main.stmts);

        // For each assigned variable, try to get its value from the VM
        for var_name in assigned_vars {
            if let Some(value) = vm.get_global(&var_name) {
                // Handle StructRef - convert to StructInstance and store in struct_instances
                if let Value::StructRef(idx) = value {
                    // StructRef variables: convert to StructInstance and store in struct_instances
                    // Also store the StructRef index in globals for quick access
                    if let Some(struct_instance) = vm.get_struct_heap().get(idx) {
                        // Use struct_name directly (Rational is defined in Pure Julia, not in program.structs)
                        if let Some(literal) = struct_instance_to_literal(
                            struct_instance,
                            &struct_instance.struct_name,
                        ) {
                            self.struct_instances.insert(var_name.clone(), literal);
                        }
                        // Store type information for type inference
                        // Save struct_name to resolve type_id from struct_table during compilation
                        self.global_struct_names
                            .insert(var_name.clone(), struct_instance.struct_name.clone());
                        // Use type_id from struct_instance as placeholder (will be resolved during compilation)
                        self.global_types
                            .insert(var_name.clone(), ValueType::Struct(struct_instance.type_id));
                    }
                    // Also save StructRef index to globals
                    self.globals.set(&var_name, value);
                } else {
                    // Infer type from value
                    let value_type = match &value {
                        Value::I64(_) => ValueType::I64,
                        Value::F64(_) => ValueType::F64,
                        Value::Str(_) => ValueType::Str,
                        Value::Array(arr_ref) => {
                            // Preserve element type for proper type inference
                            let arr = arr_ref.borrow();
                            ValueType::ArrayOf(arr.element_type())
                        }
                        // Memory → Array (Issue #2764)
                        Value::Memory(mem) => {
                            let arr = crate::vm::util::memory_to_array_ref(mem);
                            let arr_borrow = arr.borrow();
                            ValueType::ArrayOf(arr_borrow.element_type())
                        }
                        Value::NamedTuple(_) => ValueType::Tuple, // NamedTuple is a Tuple in type system
                        _ => ValueType::Any,
                    };
                    self.global_types.insert(var_name.clone(), value_type);
                    self.globals.set(&var_name, value);
                }
            }
        }
    }

    /// Store new function, struct, and module definitions.
    fn store_definitions(&mut self, program: &Program) {
        // Update functions (replace existing, add new)
        for func in &program.functions {
            if let Some(&idx) = self.function_index.get(&func.name) {
                self.functions[idx] = func.clone();
            } else {
                let idx = self.functions.len();
                self.functions.push(func.clone());
                self.function_index.insert(func.name.clone(), idx);
            }
        }

        // Update structs
        for s in &program.structs {
            if let Some(&idx) = self.struct_index.get(&s.name) {
                self.structs[idx] = s.clone();
            } else {
                let idx = self.structs.len();
                self.structs.push(s.clone());
                self.struct_index.insert(s.name.clone(), idx);
            }
        }

        // Update modules (replace existing, add new)
        for m in &program.modules {
            if let Some(&idx) = self.module_index.get(&m.name) {
                self.modules[idx] = m.clone();
            } else {
                let idx = self.modules.len();
                self.modules.push(m.clone());
                self.module_index.insert(m.name.clone(), idx);
            }
        }

        // Update usings (add new, don't replace)
        for using in &program.usings {
            if !self.usings.iter().any(|u| u.module == using.module) {
                self.usings.push(using.clone());
            }
        }
    }

    /// Reset the session, clearing all variables and definitions.
    pub fn reset(&mut self) {
        self.globals.clear();
        self.functions.clear();
        self.function_index.clear();
        self.structs.clear();
        self.struct_index.clear();
        self.modules.clear();
        self.module_index.clear();
        self.usings.clear();
        self.struct_instances.clear();
        self.ans = None;
        self.eval_count = 0;
        self.last_struct_heap.clear();
    }

    /// Get the last VM's struct heap (for resolving StructRefs in display)
    pub fn get_struct_heap(&self) -> &[StructInstance] {
        &self.last_struct_heap
    }

    /// Get the last evaluation result (ans).
    pub fn get_ans(&self) -> Option<&Value> {
        self.ans.as_ref()
    }

    /// Get all variable names in the session.
    pub fn variable_names(&self) -> Vec<String> {
        self.globals.variable_names()
    }

    /// Split input into top-level expressions.
    /// Returns a vector of (start_byte, end_byte, source_text) for each expression.
    /// If parsing fails, returns None.
    /// Uses simple heuristic splitting based on newlines (Julia REPL style).
    pub fn split_expressions(&self, input: &str) -> Option<Vec<(usize, usize, String)>> {
        // Split on newlines when outside of block structures
        // This matches Julia REPL behavior: each top-level line is evaluated separately
        let mut exprs = Vec::new();
        let mut current_start = 0;
        let mut in_block = 0i32;
        let mut in_string = false;
        let mut in_triple_string = false;
        let mut escape_next = false;
        let mut in_line_comment = false;
        let mut block_comment_depth = 0i32;
        let mut paren_depth = 0i32;
        let mut bracket_depth = 0i32;

        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let ch = chars[i];

            // Handle escape sequences in strings
            if escape_next {
                escape_next = false;
                i += 1;
                continue;
            }

            // End of line resets line comment state (but not block comment)
            if ch == '\n' {
                in_line_comment = false;
            }

            // Block comment handling (#= ... =#) - supports nesting
            if !in_string && !in_triple_string && !in_line_comment {
                // Check for block comment start (#=)
                if ch == '#' && i + 1 < chars.len() && chars[i + 1] == '=' {
                    // If we're at the start of an expression (nothing meaningful before this),
                    // we'll need to update current_start when the comment ends
                    block_comment_depth += 1;
                    i += 2;
                    continue;
                }
                // Check for block comment end (=#)
                if block_comment_depth > 0
                    && ch == '='
                    && i + 1 < chars.len()
                    && chars[i + 1] == '#'
                {
                    block_comment_depth -= 1;
                    i += 2;
                    // When we exit the outermost block comment, skip past it for expression extraction
                    if block_comment_depth == 0 {
                        // Check if everything from current_start to here is just whitespace/comments
                        let prefix: String = chars[current_start..i].iter().collect();
                        let prefix_is_just_whitespace_and_comments = prefix.trim().is_empty()
                            || prefix.trim_start().starts_with("#=")
                            || prefix.lines().all(|line| {
                                let t = line.trim();
                                t.is_empty() || t.starts_with('#') || t == "=#"
                            });
                        if prefix_is_just_whitespace_and_comments {
                            current_start = i;
                        }
                    }
                    continue;
                }
            }

            // Skip everything inside block comments
            if block_comment_depth > 0 {
                i += 1;
                continue;
            }

            // Line comment handling (skip # to end of line)
            // Only if not starting a block comment (#=)
            if !in_string && !in_triple_string && ch == '#' {
                // Already checked for #= above, so this is a line comment
                in_line_comment = true;
                i += 1;
                continue;
            }

            if in_line_comment {
                i += 1;
                continue;
            }

            // Escape sequence in strings
            if (in_string || in_triple_string) && ch == '\\' {
                escape_next = true;
                i += 1;
                continue;
            }

            // Triple-quoted string handling (""")
            if !in_string
                && i + 2 < chars.len()
                && ch == '"'
                && chars[i + 1] == '"'
                && chars[i + 2] == '"'
            {
                if in_triple_string {
                    in_triple_string = false;
                    i += 3;
                    continue;
                } else {
                    in_triple_string = true;
                    i += 3;
                    continue;
                }
            }

            // Regular string handling
            if !in_triple_string && ch == '"' {
                in_string = !in_string;
                i += 1;
                continue;
            }

            // Skip processing inside strings
            if in_string || in_triple_string {
                i += 1;
                continue;
            }

            // Track parentheses and brackets (for multi-line expressions)
            if ch == '(' {
                paren_depth += 1;
            } else if ch == ')' {
                paren_depth -= 1;
            } else if ch == '[' {
                bracket_depth += 1;
            } else if ch == ']' {
                bracket_depth -= 1;
            }

            // Track block depth - check keywords at word boundaries
            let is_keyword = |kw: &str| -> bool {
                let kw_bytes = kw.as_bytes();
                let kw_len = kw_bytes.len();
                if i + kw_len > chars.len() {
                    return false;
                }
                // Compare chars directly without String allocation
                for (j, &b) in kw_bytes.iter().enumerate() {
                    if chars[i + j] != b as char {
                        return false;
                    }
                }
                // Check not preceded by alphanumeric
                if i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
                    return false;
                }
                // Check not followed by alphanumeric
                if i + kw_len < chars.len()
                    && (chars[i + kw_len].is_alphanumeric() || chars[i + kw_len] == '_')
                {
                    return false;
                }
                true
            };

            if is_keyword("function")
                || is_keyword("if")
                || is_keyword("for")
                || is_keyword("while")
                || is_keyword("begin")
                || is_keyword("try")
                || is_keyword("module")
                || is_keyword("struct")
                || is_keyword("let")
                || is_keyword("quote")
                || is_keyword("macro")
                || is_keyword("do")
            {
                in_block += 1;
            } else if is_keyword("end") {
                in_block = (in_block - 1).max(0);
            }

            // Check for expression boundary at newline
            // Split when: outside blocks, balanced parens/brackets, at newline
            if ch == '\n' && in_block == 0 && paren_depth == 0 && bracket_depth == 0 {
                // Look ahead to see if there's more content (non-empty, non-comment line)
                let mut j = i + 1;
                // Skip blank lines
                while j < chars.len() && chars[j] == '\n' {
                    j += 1;
                }
                // Skip leading whitespace on the next line
                while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                    j += 1;
                }

                // If there's more content
                if j < chars.len() && chars[j] != '\n' {
                    // Extract the current expression (up to and including this newline)
                    let end_pos = i + 1;
                    let text: String = chars[current_start..end_pos].iter().collect();
                    let trimmed = text.trim();

                    // Check if this is a non-comment expression
                    // Filter out lines that are just comments
                    let is_just_comment = trimmed.lines().all(|line| {
                        let line_trimmed = line.trim();
                        line_trimmed.is_empty() || line_trimmed.starts_with('#')
                    });

                    if !trimmed.is_empty() && !is_just_comment {
                        // Extract only the non-comment content
                        let filtered: String = trimmed
                            .lines()
                            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
                            .collect::<Vec<_>>()
                            .join("\n");
                        if !filtered.is_empty() {
                            exprs.push((current_start, end_pos, filtered));
                        }
                    }

                    // Always advance current_start past processed content
                    current_start = end_pos;
                    // Skip the blank lines we already processed
                    while current_start < j
                        && (chars[current_start] == '\n'
                            || chars[current_start] == ' '
                            || chars[current_start] == '\t')
                    {
                        current_start += 1;
                    }
                }
            }

            i += 1;
        }

        // Add remaining content
        if current_start < chars.len() {
            let text: String = chars[current_start..].iter().collect();
            let trimmed = text.trim();

            // Check if this is a non-comment expression
            let is_just_comment = trimmed.lines().all(|line| {
                let line_trimmed = line.trim();
                line_trimmed.is_empty() || line_trimmed.starts_with('#')
            });

            if !trimmed.is_empty() && !is_just_comment {
                // Extract only the non-comment content
                let filtered: String = trimmed
                    .lines()
                    .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !filtered.is_empty() {
                    exprs.push((current_start, chars.len(), filtered));
                }
            }
        }

        // Only return if there are multiple expressions
        if exprs.len() > 1 {
            Some(exprs)
        } else {
            None
        }
    }

    /// Check if input contains multiple top-level expressions.
    pub fn has_multiple_expressions(&self, input: &str) -> bool {
        self.split_expressions(input).is_some()
    }
}
