//! Collection and resolution helpers for the compilation driver.
//!
//! These functions collect structs, functions, module info, using imports,
//! and struct literal types from the IR tree. They also handle type qualification
//! and resolution for module-scoped types.

use crate::ir::core::{Block, BuiltinOp, Expr, Function, Literal, Stmt, UsingImport};
use crate::types::JuliaType;
use std::collections::{HashMap, HashSet};

/// Recursively collect using imports from a module and its submodules.
pub(in crate::compile) fn collect_module_usings_recursive<'a>(
    module: &'a crate::ir::core::Module,
    usings: &mut Vec<&'a UsingImport>,
) {
    usings.extend(module.usings.iter());
    for submodule in &module.submodules {
        collect_module_usings_recursive(submodule, usings);
    }
}

/// Collect `public` declarations by qualified module path so early module-body
/// imports can synthesize a reflection-complete Module value before the
/// authoritative global module slot is initialized.
pub(in crate::compile) fn collect_module_publics(
    module: &crate::ir::core::Module,
    prefix: &str,
    publics: &mut HashMap<String, Vec<String>>,
) {
    let path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{prefix}.{}", module.name)
    };
    publics.insert(path.clone(), module.publics.clone());
    for submodule in &module.submodules {
        collect_module_publics(submodule, &path, publics);
    }
}

/// Recursively collect structs from a module and its submodules.
pub(in crate::compile) fn collect_module_structs<'a>(
    module: &'a crate::ir::core::Module,
    prefix: &str,
    all_structs: &mut Vec<(&'a crate::ir::core::StructDef, String)>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };
    for struct_def in &module.structs {
        all_structs.push((struct_def, module_path.clone()));
    }
    for submodule in &module.submodules {
        collect_module_structs(submodule, &module_path, all_structs);
    }
}

/// Build a `struct name -> declares inner constructor(s)` map from IR
/// definitions (Issue #10092).
///
/// The compiled `struct_defs` carried by the Base bytecode cache
/// (`StructDefInfo`) do not serialize the inner-constructor flag, so
/// cache-restore paths must recover it from the IR program. Without it,
/// `struct_table` entries rebuilt from the cache report
/// `has_inner_constructor: false` and the compiler synthesizes the field-count
/// default constructor for structs that suppress it (e.g. Base `WeakRef`,
/// whose outer constructor `WeakRef(x) = _weakref_new(x)` must run so the
/// weak cell is registered with the GC).
///
/// Covers top-level structs (bare name) and module structs (qualified
/// `Path.Name`). Resolve names with [`inner_constructor_flag_for`], which also
/// handles parametric instantiations (`Foo{Int64}` -> `Foo`).
pub(in crate::compile) fn collect_inner_constructor_flags<'a>(
    structs: impl IntoIterator<Item = &'a crate::ir::core::StructDef>,
    modules: impl IntoIterator<Item = &'a crate::ir::core::Module>,
) -> HashMap<String, bool> {
    let mut flags = HashMap::new();
    for def in structs {
        flags.insert(def.name.clone(), !def.inner_constructors.is_empty());
    }
    let mut module_structs = Vec::new();
    for module in modules {
        collect_module_structs(module, "", &mut module_structs);
    }
    for (def, module_path) in module_structs {
        flags.insert(
            format!("{}.{}", module_path, def.name),
            !def.inner_constructors.is_empty(),
        );
    }
    flags
}

/// Look up a struct's inner-constructor flag, resolving parametric
/// instantiation names (`Foo{Int64}`) through their base definition (`Foo`),
/// matching what a fresh compile records at instantiation time (Issue #10092).
pub(in crate::compile) fn inner_constructor_flag_for(
    flags: &HashMap<String, bool>,
    struct_name: &str,
) -> bool {
    if let Some(flag) = flags.get(struct_name) {
        return *flag;
    }
    if let Some(brace_idx) = struct_name.find('{') {
        if let Some(flag) = flags.get(&struct_name[..brace_idx]) {
            return *flag;
        }
    }
    false
}

pub(in crate::compile) fn collect_module_abstract_names(
    module: &crate::ir::core::Module,
    prefix: &str,
    abstract_names: &mut HashMap<String, HashSet<String>>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };
    for abstract_type in &module.abstract_types {
        abstract_names
            .entry(module_path.clone())
            .or_default()
            .insert(abstract_type.name.clone());
    }
    // Runtime-reached declarations stay inert for publication, but their
    // lexical owner is still known statically. Include their local names only
    // in this qualification map so a later module struct records `M.A` as its
    // parent without eagerly registering `A` as an active type (Issue #11686).
    abstract_names
        .entry(module_path.clone())
        .or_default()
        .extend(super::pipeline_ctx::collect_runtime_nominal_names_in_block(
            &module.body,
        ));
    for submodule in &module.submodules {
        collect_module_abstract_names(submodule, &module_path, abstract_names);
    }
}

pub(in crate::compile) fn collect_module_runtime_nominal_names(
    module: &crate::ir::core::Module,
    prefix: &str,
    names: &mut HashSet<String>,
) {
    if module.is_base_origin || module.is_package_origin {
        return;
    }
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{prefix}.{}", module.name)
    };
    names.extend(
        super::pipeline_ctx::collect_runtime_nominal_names_in_block(&module.body)
            .into_iter()
            .map(|name| format!("{module_path}.{name}")),
    );
    for submodule in &module.submodules {
        collect_module_runtime_nominal_names(submodule, &module_path, names);
    }
}

pub(in crate::compile) fn qualify_module_local_parent_type(
    parent: Option<String>,
    module_path: &str,
    module_abstract_names: &HashMap<String, HashSet<String>>,
) -> Option<String> {
    let parent = parent?;
    if parent.contains('.') {
        return Some(parent);
    }
    let family = parent.split('{').next().unwrap_or(&parent);
    if module_abstract_names
        .get(module_path)
        .is_some_and(|names| names.contains(family))
    {
        Some(format!("{}.{}", module_path, parent))
    } else {
        Some(parent)
    }
}

/// Recursively collect module info (function names, exports, constants).
pub(in crate::compile) fn collect_module_info(
    module: &crate::ir::core::Module,
    prefix: &str,
    module_functions: &mut HashMap<String, HashSet<String>>,
    module_exports: &mut HashMap<String, HashSet<String>>,
    module_constants: &mut HashMap<String, HashSet<String>>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };

    // Collect constants from module body (top-level assignments)
    let mut const_names: HashSet<String> = HashSet::new();
    collect_module_body_binding_names(&module.body, &mut const_names);
    module_constants.insert(module_path.clone(), const_names.clone());

    // Collect callable names plus direct module bindings. Despite the historical
    // field name, the import resolver uses this set as the module surface for
    // `using Module`: functions, types, type aliases, macros, submodules, and
    // module constants all need to become visible when exported.
    let mut func_names: HashSet<String> = module.functions.iter().map(|f| f.name.clone()).collect();
    func_names.extend(module.structs.iter().map(|s| s.name.clone()));
    func_names.extend(module.abstract_types.iter().map(|a| a.name.clone()));
    func_names.extend(module.primitive_types.iter().map(|p| p.name.clone()));
    func_names.extend(module.type_aliases.iter().map(|t| t.name.clone()));
    func_names.extend(module.macros.iter().map(|m| format!("@{}", m.name)));
    func_names.extend(module.submodules.iter().map(|m| m.name.clone()));
    func_names.extend(const_names);
    module_functions.insert(module_path.clone(), func_names);

    // Collect exports
    let mut export_names: HashSet<String> = module.exports.iter().cloned().collect();
    export_names.insert(module.name.clone());
    let mut known_exports = HashSet::new();
    let mut emitted_exports = export_names.clone();
    known_exports.insert(module.name.clone());
    collect_module_body_export_names(
        &module.body,
        &mut export_names,
        &mut known_exports,
        &mut emitted_exports,
        &module.name,
        &module_path,
    );
    export_names.remove(&module.name);
    module_exports.insert(module_path.clone(), export_names);

    // Recursively process submodules
    for submodule in &module.submodules {
        collect_module_info(
            submodule,
            &module_path,
            module_functions,
            module_exports,
            module_constants,
        );
    }
}

/// Register every module path in `registry`, in the exact depth-first,
/// source-declared order [`collect_module_info`] visits `module.submodules`
/// — the "module registration order" `ModuleId`'s allocation contract
/// requires (Issue #10988 Phase 2a), independent of any `HashMap` iteration.
///
/// A dedicated walk rather than a parameter threaded through
/// `collect_module_info` itself: that function has another call site
/// (`compile/cache.rs::ReplModuleMetadata::from_modules`, the REPL
/// relocatable-delta path) with no registry to populate, so widening its
/// signature would force every caller to thread one through. The
/// qualification rule (`prefix.join(".", name)`) is intentionally duplicated
/// verbatim from `collect_module_info` because registration must visit the
/// identical path set in the identical order — pinned by
/// `register_module_ids_matches_collect_module_info_paths` below, which
/// asserts the two functions produce byte-identical path sets for the same
/// module tree.
pub(in crate::compile) fn register_module_ids(
    module: &crate::ir::core::Module,
    prefix: &str,
    registry: &mut subset_julia_vm_bytecode::ModuleInternTable,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };
    registry.intern(&module_path);
    for submodule in &module.submodules {
        register_module_ids(submodule, &module_path, registry);
    }
}

/// Walk a module body in source order and record, for each name first bound
/// by a plain top-level assignment, the `definition_order` of the last
/// `Stmt::Using` marker preceding that assignment (0 when it precedes every
/// import). An import of the same name whose own `definition_order` is
/// greater conflicts with the existing binding and is ignored with a warning
/// upstream (Issue #11426). The scope-recursion rules mirror
/// `collect_module_body_binding_names` below.
pub(crate) fn collect_module_body_value_binding_positions(
    block: &Block,
    last_using_order: &mut u64,
    import_marker_starts: &HashSet<usize>,
    positions: &mut HashMap<String, u64>,
) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Using { span, .. } => {
                *last_using_order = span.definition_order;
            }
            Stmt::Assign { var, span, .. } => {
                // Lowering realizes `import M: f as g` as a synthetic
                // assignment sharing the import statement's span; it is
                // import machinery, not a user value binding.
                if import_marker_starts.contains(&span.start) {
                    continue;
                }
                positions
                    .entry(var.to_string())
                    .or_insert(*last_using_order);
            }
            Stmt::Block(inner) => collect_module_body_value_binding_positions(
                inner,
                last_using_order,
                import_marker_starts,
                positions,
            ),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_module_body_value_binding_positions(
                    then_branch,
                    last_using_order,
                    import_marker_starts,
                    positions,
                );
                if let Some(else_branch) = else_branch {
                    collect_module_body_value_binding_positions(
                        else_branch,
                        last_using_order,
                        import_marker_starts,
                        positions,
                    );
                }
            }
            Stmt::Expr { expr, .. } => collect_module_body_expr_value_binding_positions(
                expr,
                last_using_order,
                import_marker_starts,
                positions,
            ),
            _ => {}
        }
    }
}

/// The `Stmt::Using` marker span starts of a module body, used to recognize
/// lowering's synthetic rename assignments (which share the import's span).
pub(crate) fn collect_module_body_import_marker_starts(block: &Block, starts: &mut HashSet<usize>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Using { span, .. } => {
                starts.insert(span.start);
            }
            Stmt::Block(inner) => collect_module_body_import_marker_starts(inner, starts),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_module_body_import_marker_starts(then_branch, starts);
                if let Some(else_branch) = else_branch {
                    collect_module_body_import_marker_starts(else_branch, starts);
                }
            }
            _ => {}
        }
    }
}

fn collect_module_body_expr_value_binding_positions(
    expr: &Expr,
    last_using_order: &mut u64,
    import_marker_starts: &HashSet<usize>,
    positions: &mut HashMap<String, u64>,
) {
    match expr {
        Expr::AssignExpr { var, .. } => {
            positions
                .entry(var.to_string())
                .or_insert(*last_using_order);
        }
        Expr::LetBlock { bindings, body, .. } if bindings.is_empty() => {
            collect_module_body_value_binding_positions(
                body,
                last_using_order,
                import_marker_starts,
                positions,
            );
        }
        _ => {}
    }
}

pub(crate) fn collect_module_body_binding_names(block: &Block, names: &mut HashSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Assign { var, .. } => {
                names.insert(var.to_string());
            }
            Stmt::EnumDef { enum_def, .. } => {
                names.extend(enum_def.members.iter().map(|member| member.name.clone()));
            }
            Stmt::RuntimeNominalDef {
                definition: crate::ir::core::RuntimeNominalDef::Enum(enum_def),
                ..
            } => {
                // Reached module-local enum members are ordinary module
                // bindings. Recording their lexical names here makes later
                // bare reads load `M.member` while skipped definitions still
                // fail at that qualified runtime load (Issue #11733).
                names.extend(enum_def.members.iter().map(|member| member.name.clone()));
            }
            // `begin ... end` blocks introduce no new scope at module top level,
            // so their assignments are module bindings.
            Stmt::Block(inner) => collect_module_body_binding_names(inner, names),
            // `if`/`elseif`/`else` introduce no new scope at module top level, so a
            // `const`/`global` assignment in any branch is registered as a member of
            // the module — matching upstream Julia, where `module M; if true; const
            // x = 1; end; end` defines `M.x` (Issue #7917). `elseif` chains are
            // lowered as a nested `Stmt::If` inside `else_branch`, so recursing into
            // both branch bodies walks the whole chain. We deliberately do NOT
            // recurse into `for`/`while`/`let`/function bodies, which DO introduce a
            // local scope whose assignments must not leak as module bindings.
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_module_body_binding_names(then_branch, names);
                if let Some(else_branch) = else_branch {
                    collect_module_body_binding_names(else_branch, names);
                }
            }
            Stmt::Expr { expr, .. } => collect_module_body_expr_binding_names(expr, names),
            _ => {}
        }
    }
}

fn collect_module_body_expr_binding_names(expr: &Expr, names: &mut HashSet<String>) {
    match expr {
        Expr::AssignExpr { var, .. } => {
            names.insert(var.to_string());
        }
        // Macro-expanded `begin`/`quote` blocks may lower to an empty-binding
        // LetBlock at module top level. Unlike a source `let`, this wrapper does
        // not introduce a fresh binding scope, so assignments inside it remain
        // module bindings.
        Expr::LetBlock { bindings, body, .. } if bindings.is_empty() => {
            collect_module_body_binding_names(body, names);
        }
        _ => {}
    }
}

fn collect_module_body_export_names(
    block: &Block,
    names: &mut HashSet<String>,
    known_exports: &mut HashSet<String>,
    emitted_exports: &mut HashSet<String>,
    module_name: &str,
    module_path: &str,
) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Export {
                names: export_names,
                ..
            } => {
                for name in export_names {
                    known_exports.insert(name.clone());
                    if emitted_exports.insert(name.clone()) {
                        names.insert(name.clone());
                    }
                }
            }
            Stmt::Block(inner) => {
                collect_module_body_export_names(
                    inner,
                    names,
                    known_exports,
                    emitted_exports,
                    module_name,
                    module_path,
                );
            }
            Stmt::Expr { expr, .. } => {
                collect_module_body_expr_export_names(
                    expr,
                    names,
                    known_exports,
                    emitted_exports,
                    module_name,
                    module_path,
                );
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => match eval_module_export_condition(
                condition,
                known_exports,
                module_name,
                module_path,
            ) {
                Some(true) => {
                    collect_module_body_export_names(
                        then_branch,
                        names,
                        known_exports,
                        emitted_exports,
                        module_name,
                        module_path,
                    );
                }
                Some(false) => {
                    if let Some(else_branch) = else_branch {
                        collect_module_body_export_names(
                            else_branch,
                            names,
                            known_exports,
                            emitted_exports,
                            module_name,
                            module_path,
                        );
                    }
                }
                None => {}
            },
            _ => {}
        }
    }
}

fn collect_module_body_expr_export_names(
    expr: &Expr,
    names: &mut HashSet<String>,
    known_exports: &mut HashSet<String>,
    emitted_exports: &mut HashSet<String>,
    module_name: &str,
    module_path: &str,
) {
    if let Expr::LetBlock { bindings, body, .. } = expr {
        if bindings.is_empty() {
            collect_module_body_export_names(
                body,
                names,
                known_exports,
                emitted_exports,
                module_name,
                module_path,
            );
        }
    }
}

fn eval_module_export_condition(
    expr: &Expr,
    known_exports: &HashSet<String>,
    module_name: &str,
    module_path: &str,
) -> Option<bool> {
    match expr {
        Expr::Literal(Literal::Bool(value), _) => Some(*value),
        Expr::Var(name, _) if name == "true" => Some(true),
        Expr::Var(name, _) if name == "false" => Some(false),
        Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand,
            ..
        } => eval_module_export_condition(operand, known_exports, module_name, module_path)
            .map(|value| !value),
        Expr::Call { function, args, .. }
            if matches!(function.as_str(), "in" | "∈") && args.len() == 2 =>
        {
            eval_symbol_in_module_names(&args[0], &args[1], known_exports, module_name, module_path)
        }
        Expr::Call { function, args, .. }
            if matches!(function.as_str(), "∉") && args.len() == 2 =>
        {
            eval_symbol_in_module_names(&args[0], &args[1], known_exports, module_name, module_path)
                .map(|value| !value)
        }
        Expr::Builtin {
            name: BuiltinOp::In,
            args,
            ..
        } if args.len() == 2 => {
            eval_symbol_in_module_names(&args[0], &args[1], known_exports, module_name, module_path)
        }
        _ => None,
    }
}

fn eval_symbol_in_module_names(
    needle: &Expr,
    haystack: &Expr,
    known_exports: &HashSet<String>,
    module_name: &str,
    module_path: &str,
) -> Option<bool> {
    let symbol = expr_symbol_name(needle)?;
    let haystack_module = names_call_module_name(haystack)?;
    if haystack_module != module_name && haystack_module != module_path {
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

/// Recursively collect functions from a module and its submodules, tracking module paths.
pub(in crate::compile) fn collect_module_functions<'a>(
    module: &'a crate::ir::core::Module,
    prefix: &str,
    all_functions: &mut Vec<(&'a Function, Option<String>)>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };
    for func in &module.functions {
        all_functions.push((func, Some(module_path.clone())));
    }
    for submodule in &module.submodules {
        collect_module_functions(submodule, &module_path, all_functions);
    }
}

/// Collect functions defined inside statement blocks (Stmt::FunctionDef).
/// These are inline function definitions, e.g., inside @testset bodies.
/// Returns (Function, Option<parent_function_name>) to track nested functions.
pub(in crate::compile) fn collect_block_functions(
    block: &Block,
    functions: &mut Vec<(Function, Option<String>)>,
    parent_func_name: Option<&str>,
) {
    collect_block_functions_with_new_authority(block, functions, parent_func_name, None);
}

/// Collect lexical descendants while carrying a struct helper's privileged
/// `new` owner. Runtime `@eval` definitions are a hard scope boundary and clear
/// this authority for both the global function and its descendants (#11197).
pub(in crate::compile) fn collect_block_functions_with_new_authority(
    block: &Block,
    functions: &mut Vec<(Function, Option<String>)>,
    parent_func_name: Option<&str>,
    new_struct_name: Option<&str>,
) {
    for stmt in &block.stmts {
        collect_stmt_functions_with_new_authority(
            stmt,
            functions,
            parent_func_name,
            new_struct_name,
        );
    }
}

pub(in crate::compile) fn collect_expr_functions(
    expr: &Expr,
    functions: &mut Vec<(Function, Option<String>)>,
    parent_func_name: Option<&str>,
) {
    collect_expr_functions_with_new_authority(expr, functions, parent_func_name, None);
}

fn collect_expr_functions_with_new_authority(
    expr: &Expr,
    functions: &mut Vec<(Function, Option<String>)>,
    parent_func_name: Option<&str>,
    new_struct_name: Option<&str>,
) {
    match expr {
        Expr::LetBlock { bindings, body, .. } => {
            // Binding values are evaluated expressions too and may contain a
            // nested FunctionDef container (for example, a context-aware arrow
            // used as the callable of `|>`). Ignoring bindings hides that
            // function from bytecode compilation; chained pipes can hide the
            // left pipe recursively the same way. Traverse values before the
            // body to mirror Julia's evaluation order and the other IR walkers
            // (Issue #11030).
            for (_, value) in bindings {
                collect_expr_functions_with_new_authority(
                    value,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
            collect_block_functions_with_new_authority(
                body,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Expr::Call { args, kwargs, .. } => {
            for arg in args {
                collect_expr_functions_with_new_authority(
                    arg,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
            for (_, value) in kwargs {
                collect_expr_functions_with_new_authority(
                    value,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                collect_expr_functions_with_new_authority(
                    arg,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_expr_functions_with_new_authority(
                    arg,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
            for (_, value) in kwargs {
                collect_expr_functions_with_new_authority(
                    value,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_expr_functions_with_new_authority(
                left,
                functions,
                parent_func_name,
                new_struct_name,
            );
            collect_expr_functions_with_new_authority(
                right,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Expr::UnaryOp { operand, .. } => {
            collect_expr_functions_with_new_authority(
                operand,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Expr::Index { array, indices, .. } => {
            collect_expr_functions_with_new_authority(
                array,
                functions,
                parent_func_name,
                new_struct_name,
            );
            for index in indices {
                collect_expr_functions_with_new_authority(
                    index,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_expr_functions_with_new_authority(
                start,
                functions,
                parent_func_name,
                new_struct_name,
            );
            if let Some(step) = step {
                collect_expr_functions_with_new_authority(
                    step,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
            collect_expr_functions_with_new_authority(
                stop,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            collect_expr_functions_with_new_authority(
                body,
                functions,
                parent_func_name,
                new_struct_name,
            );
            collect_expr_functions_with_new_authority(
                iter,
                functions,
                parent_func_name,
                new_struct_name,
            );
            if let Some(filter) = filter {
                collect_expr_functions_with_new_authority(
                    filter,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            collect_expr_functions_with_new_authority(
                body,
                functions,
                parent_func_name,
                new_struct_name,
            );
            for (_, iter) in iterations {
                collect_expr_functions_with_new_authority(
                    iter,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
            if let Some(filter) = filter {
                collect_expr_functions_with_new_authority(
                    filter,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::FieldAccess { object, .. } => {
            collect_expr_functions_with_new_authority(
                object,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            for expr in [condition.as_ref(), then_expr.as_ref(), else_expr.as_ref()] {
                collect_expr_functions_with_new_authority(
                    expr,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
            for elem in elements {
                collect_expr_functions_with_new_authority(
                    elem,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_expr_functions_with_new_authority(
                    value,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_expr_functions_with_new_authority(
                key,
                functions,
                parent_func_name,
                new_struct_name,
            );
            collect_expr_functions_with_new_authority(
                value,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_expr_functions_with_new_authority(
                    key,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
                collect_expr_functions_with_new_authority(
                    value,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                collect_expr_functions_with_new_authority(
                    part,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::New { args, .. } => {
            for arg in args {
                collect_expr_functions_with_new_authority(
                    arg,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                collect_expr_functions_with_new_authority(
                    base_expr,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
            for arg in type_args {
                collect_expr_functions_with_new_authority(
                    arg,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Expr::QuoteLiteral { constructor, .. } => {
            collect_expr_functions_with_new_authority(
                constructor,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Expr::AssignExpr { value, .. }
        | Expr::ReturnExpr {
            value: Some(value), ..
        } => {
            collect_expr_functions_with_new_authority(
                value,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        _ => {}
    }
}

pub(in crate::compile) fn collect_stmt_functions(
    stmt: &Stmt,
    functions: &mut Vec<(Function, Option<String>)>,
    parent_func_name: Option<&str>,
) {
    collect_stmt_functions_with_new_authority(stmt, functions, parent_func_name, None);
}

fn collect_stmt_functions_with_new_authority(
    stmt: &Stmt,
    functions: &mut Vec<(Function, Option<String>)>,
    parent_func_name: Option<&str>,
    new_struct_name: Option<&str>,
) {
    match stmt {
        Stmt::FunctionDef { func, .. } => {
            let mut nested = (*func.clone()).clone();
            if let Some(struct_name) = new_struct_name {
                nested.new_struct_name = Some(struct_name.to_string());
            }
            functions.push((nested, parent_func_name.map(|s| s.to_string())));
            // Issue #1744: Recursively collect nested functions from this function's body
            // For 3+ levels of nesting, use qualified name as new parent
            let qualified_parent = if let Some(parent) = parent_func_name {
                format!("{}#{}", parent, func.name)
            } else {
                func.name.clone()
            };
            collect_block_functions_with_new_authority(
                &func.body,
                functions,
                Some(&qualified_parent),
                new_struct_name,
            );
        }
        Stmt::EvalFunctionDef { func, .. } => {
            let mut evaluated = (*func.clone()).clone();
            evaluated.new_struct_name = None;
            functions.push((evaluated, None));
            collect_block_functions_with_new_authority(
                &func.body,
                functions,
                Some(&func.name),
                None,
            );
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            collect_expr_functions_with_new_authority(
                start,
                functions,
                parent_func_name,
                new_struct_name,
            );
            collect_expr_functions_with_new_authority(
                end,
                functions,
                parent_func_name,
                new_struct_name,
            );
            if let Some(step) = step {
                collect_expr_functions_with_new_authority(
                    step,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
            collect_block_functions_with_new_authority(
                body,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            collect_expr_functions_with_new_authority(
                iterable,
                functions,
                parent_func_name,
                new_struct_name,
            );
            collect_block_functions_with_new_authority(
                body,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_expr_functions_with_new_authority(
                condition,
                functions,
                parent_func_name,
                new_struct_name,
            );
            collect_block_functions_with_new_authority(
                body,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            collect_block_functions_with_new_authority(
                body,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_expr_functions_with_new_authority(
                condition,
                functions,
                parent_func_name,
                new_struct_name,
            );
            collect_block_functions_with_new_authority(
                then_branch,
                functions,
                parent_func_name,
                new_struct_name,
            );
            if let Some(else_block) = else_branch {
                collect_block_functions_with_new_authority(
                    else_block,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_block_functions_with_new_authority(
                try_block,
                functions,
                parent_func_name,
                new_struct_name,
            );
            if let Some(block) = catch_block {
                collect_block_functions_with_new_authority(
                    block,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
            if let Some(block) = else_block {
                collect_block_functions_with_new_authority(
                    block,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
            if let Some(block) = finally_block {
                collect_block_functions_with_new_authority(
                    block,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
        }
        Stmt::Block(block) => {
            collect_block_functions_with_new_authority(
                block,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        // Also check expressions for LetBlock (from macro-expanded begin blocks)
        Stmt::Expr { expr, .. } => {
            collect_expr_functions_with_new_authority(
                expr,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Stmt::Assign { value, .. } => {
            collect_expr_functions_with_new_authority(
                value,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        // Index/field/dict assignments also carry value (and index/key) expressions
        // that may embed lambdas. Without recursing here, a lambda in the RHS of
        // `xs[i] = map(x -> ..., xs[i])` (or its index/field/dict-key variants) is
        // compiled as a function value but its generated function is never
        // registered, failing at runtime with `Function '...__lambda_nested_...'
        // not found` (Issue #7615). Mirror the AOT call-graph traversal.
        Stmt::AddAssign { value, .. } => {
            collect_expr_functions_with_new_authority(
                value,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Stmt::DictAssign { key, value, .. } => {
            collect_expr_functions_with_new_authority(
                key,
                functions,
                parent_func_name,
                new_struct_name,
            );
            collect_expr_functions_with_new_authority(
                value,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Stmt::IndexAssign { indices, value, .. } => {
            for index in indices {
                collect_expr_functions_with_new_authority(
                    index,
                    functions,
                    parent_func_name,
                    new_struct_name,
                );
            }
            collect_expr_functions_with_new_authority(
                value,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Stmt::FieldAssign { value, .. } | Stmt::DestructuringAssign { value, .. } => {
            collect_expr_functions_with_new_authority(
                value,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        // Recurse into return values so that FunctionDefs embedded in LetBlocks inside
        // return statements are discovered (e.g. partial-apply lambdas: Issue #3119).
        Stmt::Return {
            value: Some(expr), ..
        } => {
            collect_expr_functions_with_new_authority(
                expr,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Stmt::Test { condition, .. } => {
            collect_expr_functions_with_new_authority(
                condition,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Stmt::TestThrows { expr, .. } => {
            collect_expr_functions_with_new_authority(
                expr,
                functions,
                parent_func_name,
                new_struct_name,
            );
        }
        Stmt::Return { value: None, .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::LocalDecl { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. }
        | Stmt::RuntimeNominalDef { .. }
        | Stmt::Global { .. } => {}
    }
}

/// Collect named function definitions that live inside a `let` or `@testset`
/// at a module's top-level body (Issue #9942, follow-up #10073).
///
/// At Main top level, functions inside a top-level `let`/`@testset` are
/// collected by `collect_top_level_inline_functions` scanning `opt_main.stmts`
/// with `collect_stmt_functions`, which descends through `Stmt::Block` and
/// `Expr::LetBlock` UNCONDITIONALLY — including the synthetic `LetBlock`s that
/// lowering wraps do-block (`__do_block_N`, `lower_do_call_as_nested`) and
/// generator-body (`__gen_body_N`, `desugar_simple_generator`) helpers in.
/// Module bodies were never scanned this way: a helper defined in a
/// module-level `@testset` (Issue #9942, PR #9950) or a plain `let` (Issue
/// #10073) was never registered, so a call to it failed with `Unknown
/// function`.
///
/// `Test.@testset` macro-expands to `let; _testset_begin!(...); <body>;
/// _testset_end!(); end`, so by collection time a testset is an
/// `Expr::LetBlock` in the module body — indistinguishable in shape from a
/// plain user `let` or a compiler-synthesized do-block/generator wrapper. We
/// therefore descend into EVERY module-body `LetBlock`, mirroring the
/// Main-level walk exactly (no marker gate, Issue #10073): the lifted-helper
/// names (`__lambda_*`/`__do_block_*`/`__gen_body_*`) that the Main-level walk
/// already finds unconditionally are equally safe to find here, because they
/// are never registered a second, independent way for module bodies either —
/// do-block/generator lowering leaves the `Stmt::FunctionDef` in place inside
/// the synthetic `LetBlock` it builds (see `lower_do_call_as_nested`,
/// `desugar_simple_generator`); only call-argument arrow lambdas are lifted
/// out to the separate `LambdaContext::lifted_functions` side list, and those
/// never leave a nested `Stmt::FunctionDef` behind for this walk to
/// re-discover. The module walk exhaustively classifies every `Stmt` variant:
/// expression fields use the shared recursive expression visitor, hard-scope
/// bodies use the ordinary block collector, and transparent `Block`/`If`
/// bodies keep module-body classification. A future statement variant cannot
/// compile until its collection behavior is classified (Issue #10346).
///
/// Every function found directly in one of these module-body scopes — i.e.
/// with no enclosing named-function parent (`parent_func_name == None`) — is
/// recorded in `module_scope_overrides` under its **collection index** (its
/// eventual position in the final `inline_functions` vec assembled by
/// `collect_top_level_inline_functions`), so `build_function_universe` can
/// register it at the enclosing MODULE's scope instead of `None`/Main (Issue
/// #10073): otherwise a reference from inside the collected helper to a
/// module-scope global raises `UndefVarError`. A function nested one level
/// deeper (defined inside one of these collected helpers) still resolves its
/// module scope through the existing `function_module_paths` parent-name
/// chain once its direct parent's scope is corrected here.
///
/// Keying by index (not bare name, Issue #10214/#10236) is required because
/// two different modules' let/testset roots — or a module-body root and an
/// unrelated Main-level `let` root — can share the same bare function name;
/// a name-keyed map would let whichever was collected last silently win for
/// every same-named root, corrupting the OTHER root's scope resolution.
fn collect_module_body_let_functions(
    block: &Block,
    module_path: &str,
    functions: &mut Vec<(Function, Option<String>)>,
    module_scope_overrides: &mut HashMap<usize, String>,
) {
    for stmt in &block.stmts {
        match stmt {
            // Ordinary direct module functions already live in
            // `module.functions`. Runtime `@eval` definitions do not, but
            // collecting them as lexical helper roots is also incorrect;
            // their source-order module registration is tracked by #10874.
            Stmt::FunctionDef { .. } | Stmt::EvalFunctionDef { .. } => {}
            Stmt::Expr { expr, .. }
            | Stmt::Assign { value: expr, .. }
            | Stmt::AddAssign { value: expr, .. }
            | Stmt::FieldAssign { value: expr, .. }
            | Stmt::DestructuringAssign { value: expr, .. } => {
                collect_module_scoped_expr_functions(
                    expr,
                    module_path,
                    functions,
                    module_scope_overrides,
                );
            }
            Stmt::DictAssign { key, value, .. } => {
                for expr in [key, value] {
                    collect_module_scoped_expr_functions(
                        expr,
                        module_path,
                        functions,
                        module_scope_overrides,
                    );
                }
            }
            Stmt::IndexAssign { indices, value, .. } => {
                for expr in indices.iter().chain(std::iter::once(value)) {
                    collect_module_scoped_expr_functions(
                        expr,
                        module_path,
                        functions,
                        module_scope_overrides,
                    );
                }
            }
            Stmt::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                for expr in [Some(start), Some(end), step.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    collect_module_scoped_expr_functions(
                        expr,
                        module_path,
                        functions,
                        module_scope_overrides,
                    );
                }
                collect_module_scoped_functions(
                    body,
                    module_path,
                    functions,
                    module_scope_overrides,
                );
            }
            Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
                collect_module_scoped_expr_functions(
                    iterable,
                    module_path,
                    functions,
                    module_scope_overrides,
                );
                collect_module_scoped_functions(
                    body,
                    module_path,
                    functions,
                    module_scope_overrides,
                );
            }
            Stmt::While {
                condition, body, ..
            } => {
                collect_module_scoped_expr_functions(
                    condition,
                    module_path,
                    functions,
                    module_scope_overrides,
                );
                collect_module_scoped_functions(
                    body,
                    module_path,
                    functions,
                    module_scope_overrides,
                );
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                collect_module_scoped_expr_functions(
                    condition,
                    module_path,
                    functions,
                    module_scope_overrides,
                );
                // Module-level `if` introduces no lexical scope. Recurse with
                // module-body classification so genuine branch definitions are
                // not mislabeled as `let`/`@testset` local roots.
                collect_module_body_let_functions(
                    then_branch,
                    module_path,
                    functions,
                    module_scope_overrides,
                );
                if let Some(else_branch) = else_branch {
                    collect_module_body_let_functions(
                        else_branch,
                        module_path,
                        functions,
                        module_scope_overrides,
                    );
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                for body in [
                    Some(try_block),
                    catch_block.as_ref(),
                    else_block.as_ref(),
                    finally_block.as_ref(),
                ]
                .into_iter()
                .flatten()
                {
                    collect_module_scoped_functions(
                        body,
                        module_path,
                        functions,
                        module_scope_overrides,
                    );
                }
            }
            Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
                collect_module_scoped_functions(
                    body,
                    module_path,
                    functions,
                    module_scope_overrides,
                );
            }
            Stmt::Block(body) => collect_module_body_let_functions(
                body,
                module_path,
                functions,
                module_scope_overrides,
            ),
            Stmt::Return {
                value: Some(expr), ..
            }
            | Stmt::Test {
                condition: expr, ..
            } => collect_module_scoped_expr_functions(
                expr,
                module_path,
                functions,
                module_scope_overrides,
            ),
            Stmt::TestThrows { expr, .. } => collect_module_scoped_expr_functions(
                expr,
                module_path,
                functions,
                module_scope_overrides,
            ),
            Stmt::Return { value: None, .. }
            | Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::Meta { .. }
            | Stmt::LocalDecl { .. }
            | Stmt::Using { .. }
            | Stmt::Export { .. }
            | Stmt::Label { .. }
            | Stmt::Goto { .. }
            | Stmt::EnumDef { .. }
            | Stmt::RuntimeNominalDef { .. }
            | Stmt::Global { .. } => {}
        }
    }
}

/// Walk a module-body expression for nested helper functions and tag direct
/// discoveries as module-scoped. This covers synthetic generator/comprehension
/// `LetBlock`s at any expression-bearing statement position, including call
/// arguments such as `sum(i*i for i in 1:3)`.
///
/// `functions` is the SAME vec `collect_top_level_inline_functions` returns
/// as `inline_functions`, so `start + offset` is each newly-collected
/// function's final index there — the identity `module_scope_overrides` is
/// keyed by (Issue #10214/#10236, see `collect_module_body_let_functions`).
fn collect_module_scoped_expr_functions(
    expr: &Expr,
    module_path: &str,
    functions: &mut Vec<(Function, Option<String>)>,
    module_scope_overrides: &mut HashMap<usize, String>,
) {
    let start = functions.len();
    collect_expr_functions(expr, functions, None);
    tag_new_module_scoped_roots(start, module_path, functions, module_scope_overrides);
}

fn collect_module_scoped_functions(
    block: &Block,
    module_path: &str,
    functions: &mut Vec<(Function, Option<String>)>,
    module_scope_overrides: &mut HashMap<usize, String>,
) {
    let start = functions.len();
    collect_block_functions(block, functions, None);
    tag_new_module_scoped_roots(start, module_path, functions, module_scope_overrides);
}

fn tag_new_module_scoped_roots(
    start: usize,
    module_path: &str,
    functions: &[(Function, Option<String>)],
    module_scope_overrides: &mut HashMap<usize, String>,
) {
    for (offset, (_func, parent)) in functions[start..].iter().enumerate() {
        if parent.is_none() {
            module_scope_overrides
                .entry(start + offset)
                .or_insert_with(|| module_path.to_string());
        }
    }
}

/// Recursively collect functions from module function bodies.
///
/// Nested functions collected from a module's OWN named top-level functions
/// (`for func in &module.functions`) are given a MODULE-QUALIFIED parent
/// identity (`"Module.path.func_name"`, not the bare `func_name`) so that two
/// different modules defining a same-named top-level function (e.g. both
/// declaring `function outer() ... end`) do not collide when their nested
/// helpers' qualified names (`"<parent>#<nested>"`) are later used as
/// `function_indices`/`closure_captures`/method-table keys (Issue #10214):
/// a bare-keyed parent identity would let the two modules' distinct
/// `outer#helper` bodies dedup-collide into a single method-table entry, so
/// EITHER module's call to its own nested helper could silently run the
/// OTHER module's helper body.
pub(in crate::compile) fn collect_from_module(
    module: &crate::ir::core::Module,
    prefix: &str,
    inline_functions: &mut Vec<(Function, Option<String>)>,
    module_scope_overrides: &mut HashMap<usize, String>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };
    // Named functions defined inside a `let`/`@testset` at module scope (Issue
    // #9942, #10073) must be collected too, mirroring how
    // `collect_top_level_inline_functions` scans the Main block.
    collect_module_body_let_functions(
        &module.body,
        &module_path,
        inline_functions,
        module_scope_overrides,
    );
    for func in &module.functions {
        // Module-qualified parent identity (Issue #10214) — see doc comment above.
        let qualified_parent = format!("{}.{}", module_path, func.name);
        collect_block_functions_with_new_authority(
            &func.body,
            inline_functions,
            Some(&qualified_parent),
            func.new_struct_name.as_deref(),
        );
    }
    for submodule in &module.submodules {
        collect_from_module(
            submodule,
            &module_path,
            inline_functions,
            module_scope_overrides,
        );
    }
}

/// Pre-instantiate parametric struct types from Literal::Struct expressions in main block.
/// This ensures types like Complex{Float64} (from `im` literal) are in struct_table
/// BEFORE type inference runs for proper dispatch.
pub(in crate::compile) fn collect_struct_literal_types(
    stmts: &[Stmt],
    struct_names: &mut HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { value, .. } => {
                collect_struct_literal_types_from_expr(value, struct_names)
            }
            Stmt::Expr { expr, .. } => collect_struct_literal_types_from_expr(expr, struct_names),
            Stmt::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                collect_struct_literal_types_from_expr(start, struct_names);
                collect_struct_literal_types_from_expr(end, struct_names);
                if let Some(s) = step {
                    collect_struct_literal_types_from_expr(s, struct_names);
                }
                collect_struct_literal_types(&body.stmts, struct_names);
            }
            Stmt::ForEach { iterable, body, .. } => {
                collect_struct_literal_types_from_expr(iterable, struct_names);
                collect_struct_literal_types(&body.stmts, struct_names);
            }
            Stmt::While {
                condition, body, ..
            } => {
                collect_struct_literal_types_from_expr(condition, struct_names);
                collect_struct_literal_types(&body.stmts, struct_names);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                collect_struct_literal_types_from_expr(condition, struct_names);
                collect_struct_literal_types(&then_branch.stmts, struct_names);
                if let Some(eb) = else_branch {
                    collect_struct_literal_types(&eb.stmts, struct_names);
                }
            }
            Stmt::Return {
                value: Some(expr), ..
            } => collect_struct_literal_types_from_expr(expr, struct_names),
            _ => {}
        }
    }
}

pub(in crate::compile) fn collect_struct_literal_types_from_expr(
    expr: &Expr,
    struct_names: &mut HashSet<String>,
) {
    match expr {
        Expr::Literal(Literal::Struct(name, _), _) => {
            struct_names.insert(name.clone());
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_struct_literal_types_from_expr(left, struct_names);
            collect_struct_literal_types_from_expr(right, struct_names);
        }
        Expr::UnaryOp { operand, .. } => {
            collect_struct_literal_types_from_expr(operand, struct_names);
        }
        Expr::Call { args, kwargs, .. } => {
            for arg in args {
                collect_struct_literal_types_from_expr(arg, struct_names);
            }
            for (_, arg) in kwargs {
                collect_struct_literal_types_from_expr(arg, struct_names);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_struct_literal_types_from_expr(array, struct_names);
            for idx in indices {
                collect_struct_literal_types_from_expr(idx, struct_names);
            }
        }
        Expr::FieldAccess { object, .. } => {
            collect_struct_literal_types_from_expr(object, struct_names);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_struct_literal_types_from_expr(condition, struct_names);
            collect_struct_literal_types_from_expr(then_expr, struct_names);
            collect_struct_literal_types_from_expr(else_expr, struct_names);
        }
        Expr::ArrayLiteral { elements, .. } => {
            for elem in elements {
                collect_struct_literal_types_from_expr(elem, struct_names);
            }
        }
        Expr::TupleLiteral { elements, .. } => {
            for elem in elements {
                collect_struct_literal_types_from_expr(elem, struct_names);
            }
        }
        _ => {}
    }
}

/// Collect module-level using statements to support module-local imports.
pub(in crate::compile) fn collect_module_usings(
    module: &crate::ir::core::Module,
    prefix: &str,
    module_usings: &mut HashMap<String, Vec<UsingImport>>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };

    // Collect using statements from module.usings field (preserve full UsingImport info)
    module_usings.insert(module_path.clone(), module.usings.clone());

    for submodule in &module.submodules {
        collect_module_usings(submodule, &module_path, module_usings);
    }
}

/// Record whether each declared module sees the complete Base export set.
/// Ordinary modules receive it implicitly; baremodules receive it only from a
/// non-selective `using Base`. Runtime module reflection consumes this owner
/// metadata instead of inferring visibility from the shared bare function
/// registry (Issue #11410).
pub(in crate::compile) fn collect_module_base_exports_visibility(
    module: &crate::ir::core::Module,
    prefix: &str,
    visibility: &mut HashMap<String, bool>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{prefix}.{}", module.name)
    };
    let explicitly_uses_all_base_exports = module.usings.iter().any(|using_import| {
        !using_import.is_import
            && !using_import.is_relative
            && using_import.module == "Base"
            && using_import.symbols.is_none()
    });
    visibility.insert(
        module_path.clone(),
        !module.is_bare || explicitly_uses_all_base_exports,
    );
    for submodule in &module.submodules {
        collect_module_base_exports_visibility(submodule, &module_path, visibility);
    }
}

/// Record whether each declaration receives Julia's implicit per-module
/// `eval` and `include` bindings. This is declaration provenance, not import
/// visibility: `baremodule; using Base` remains false (Issue #11410).
pub(in crate::compile) fn collect_module_implicit_standard_bindings(
    module: &crate::ir::core::Module,
    prefix: &str,
    visibility: &mut HashMap<String, bool>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{prefix}.{}", module.name)
    };
    visibility.insert(module_path.clone(), !module.is_bare);
    for submodule in &module.submodules {
        collect_module_implicit_standard_bindings(submodule, &module_path, visibility);
    }
}

/// Qualify struct type names for module functions.
/// When a function is defined in a module (e.g., Dates), its parameter types like "Quarter"
/// should be qualified to "Dates.Quarter" to match the struct instances.
pub(in crate::compile) fn qualify_type_for_module(
    jt: JuliaType,
    module_path: Option<&String>,
    module_struct_names: &HashMap<String, HashSet<String>>,
) -> JuliaType {
    match (&jt, module_path) {
        (JuliaType::Struct(name), Some(path)) => {
            // Check if this struct name is defined in the module
            if let Some(structs) = module_struct_names.get(path) {
                // Handle parametric types like "Point{Int64}" - extract base name
                let base_name = if let Some(brace_idx) = name.find('{') {
                    &name[..brace_idx]
                } else {
                    name.as_str()
                };
                if structs.contains(base_name) {
                    // Qualify the full name (including type params)
                    return JuliaType::Struct(format!("{}.{}", path, name));
                }
            }
            jt
        }
        (JuliaType::TypeOf(inner), _) => {
            let qualified_inner =
                qualify_type_for_module(inner.as_ref().clone(), module_path, module_struct_names);
            JuliaType::TypeOf(Box::new(qualified_inner))
        }
        // Recursively qualify element types in VectorOf
        (JuliaType::VectorOf(elem), _) => {
            let qualified_elem =
                qualify_type_for_module(elem.as_ref().clone(), module_path, module_struct_names);
            JuliaType::VectorOf(Box::new(qualified_elem))
        }
        (JuliaType::MatrixOf(elem), _) => {
            let qualified_elem =
                qualify_type_for_module(elem.as_ref().clone(), module_path, module_struct_names);
            JuliaType::MatrixOf(Box::new(qualified_elem))
        }
        (JuliaType::TupleOf(types), _) => JuliaType::TupleOf(
            types
                .iter()
                .cloned()
                .map(|ty| qualify_type_for_module(ty, module_path, module_struct_names))
                .collect(),
        ),
        (JuliaType::Union(types), _) => JuliaType::Union(
            types
                .iter()
                .cloned()
                .map(|ty| qualify_type_for_module(ty, module_path, module_struct_names))
                .collect(),
        ),
        // A module-local struct that SHADOWS a builtin type name (`struct
        // Array` / `struct Dict` inside a module) parses to the builtin's
        // dedicated JuliaType variant, not `Struct(name)`, so the arm above
        // never sees it and the signature stayed bare — dispatch then relied
        // on the match-time bare-vs-qualified family collapse, which also
        // leaked `isa(user_value, Base.Array)` (Issues #11388/#11395).
        // Upstream lexical scoping makes the local declaration win inside its
        // module, so rewrite the annotation to the qualified module-owned
        // struct whenever the module really declares that name.
        (_, Some(path)) => {
            let name = jt.name();
            if !name.contains('.')
                && !name.contains('{')
                && module_struct_names
                    .get(path)
                    .is_some_and(|structs| structs.contains(name.as_ref()))
            {
                return JuliaType::Struct(format!("{path}.{name}"));
            }
            jt
        }
        _ => jt,
    }
}

/// Convert Struct types to AbstractUser when the type is actually an abstract type.
pub(in crate::compile) fn resolve_abstract_type(
    jt: JuliaType,
    abstract_type_parents: &HashMap<String, Option<String>>,
) -> JuliaType {
    if let JuliaType::Struct(name) = &jt {
        // Extract base name (without type params) for lookup
        let base_name = name.find('{').map(|idx| &name[..idx]).unwrap_or(name);
        if name.contains('{')
            && matches!(
                base_name,
                "AbstractArray"
                    | "AbstractVector"
                    | "AbstractMatrix"
                    | "AbstractRange"
                    | "AbstractUnitRange"
                    | "OrdinalRange"
            )
        {
            return jt;
        }
        // A module-qualified abstract annotation (`f(s::M.Shape)` written from
        // outside the module) parses to `Struct("M.Shape")`, but module abstract
        // types are registered under their *bare* name (`Shape`) in
        // `abstract_type_parents`. Strip the module prefix before the lookup so
        // the qualified annotation is reclassified to `AbstractUser("Shape")` and
        // dispatches identically to the unqualified `f(s::Shape)` form — module
        // qualification is not part of type identity (Issue #7302).
        let lookup_name = base_name.rsplit('.').next().unwrap_or(base_name);
        if let Some(parent) = abstract_type_parents.get(lookup_name) {
            // This is an abstract type - convert to AbstractUser.
            //
            // An abstract supertype parameterized by VALUE or CONCRETE type
            // parameters (`AbsM{2,2,T}`, `Container{Int64}`) must keep those
            // parameters in the carried name so dispatch can distinguish sibling
            // specializations when the argument is a concrete subtype. The
            // historical projection dropped every parameter to the bare family
            // name, collapsing specializations so the last-defined one always
            // won (Issue #7960). Pure type-variable lists (`AbstractDict{K,V}`)
            // keep the bare-family representation the rest of the dispatcher
            // already handles.
            let stored_name = match name.find('{') {
                Some(open) if name_has_dispatch_relevant_param(&name[open..]) => {
                    format!("{lookup_name}{}", &name[open..])
                }
                _ => lookup_name.to_string(),
            };
            return JuliaType::AbstractUser(stored_name, parent.clone());
        }
    }
    jt
}

/// Whether a `{...}` parameter list spells a dispatch-relevant parameter that
/// must stay attached to a parametric abstract family. Value literals and
/// concrete type names are relevant; pure type-variable lists (`{K,V}`,
/// `{T<:Real}`) are not.
fn name_has_dispatch_relevant_param(params_suffix: &str) -> bool {
    let inner = params_suffix
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .unwrap_or(params_suffix);
    split_top_level_type_args(inner)
        .into_iter()
        .any(param_token_is_dispatch_relevant)
}

fn param_token_is_dispatch_relevant(tok: &str) -> bool {
    let tok = tok.trim();
    if tok.parse::<i128>().is_ok() || tok == "true" || tok == "false" {
        return true;
    }
    if tok.contains('{') || tok.contains('.') {
        return true;
    }

    let bare = tok
        .split_once("<:")
        .or_else(|| tok.split_once(">:"))
        .map(|(name, _)| name.trim())
        .unwrap_or(tok);
    if bare != tok {
        return false;
    }

    is_builtin_concrete_dispatch_type(bare) || !is_likely_typevar_name(bare)
}

fn is_builtin_concrete_dispatch_type(name: &str) -> bool {
    matches!(
        name,
        "Any"
            | "Number"
            | "Real"
            | "Integer"
            | "Signed"
            | "Unsigned"
            | "AbstractFloat"
            | "Bool"
            | "Int"
            | "Int8"
            | "Int16"
            | "Int32"
            | "Int64"
            | "Int128"
            | "UInt"
            | "UInt8"
            | "UInt16"
            | "UInt32"
            | "UInt64"
            | "UInt128"
            | "Float16"
            | "Float32"
            | "Float64"
            | "BigInt"
            | "BigFloat"
            | "String"
            | "Symbol"
            | "Char"
            | "Nothing"
            | "Missing"
            | "Function"
            | "Type"
            | "DataType"
            | "Tuple"
            | "Pair"
            | "Complex"
            | "Rational"
            | "Vector"
            | "Matrix"
            | "Array"
    )
}

fn is_likely_typevar_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && name.chars().count() == 1
}

/// Split a comma-separated parametric argument list, respecting `{...}` nesting
/// so `Tuple{N},T` yields `["Tuple{N}", "T"]`.
fn split_top_level_type_args(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, ch) in inner.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = inner[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

/// Resolve type aliases in function parameter types (Issue #2527).
/// When `const IntWrapper = Wrapper{Int64}` is defined, a parameter annotation
/// `f(::IntWrapper)` should resolve to the target type `Wrapper{Int64}` for dispatch.
#[cfg(test)]
pub(in crate::compile) fn resolve_type_alias(
    jt: JuliaType,
    type_aliases: &HashMap<String, String>,
) -> JuliaType {
    if let JuliaType::Struct(ref name) = jt {
        if let Some(target) = type_aliases.get(name.as_str()) {
            return JuliaType::from_name_or_struct(target);
        }
    }
    jt
}

#[cfg(test)]
mod tests {
    use super::*;

    // === qualify_type_for_module ===

    #[test]
    fn test_qualify_type_for_module_known_struct() {
        let mut module_structs = HashMap::new();
        let mut dates_structs = HashSet::new();
        dates_structs.insert("Quarter".to_string());
        module_structs.insert("Dates".to_string(), dates_structs);

        let result = qualify_type_for_module(
            JuliaType::Struct("Quarter".to_string()),
            Some(&"Dates".to_string()),
            &module_structs,
        );
        assert_eq!(result, JuliaType::Struct("Dates.Quarter".to_string()));
    }

    #[test]
    fn test_qualify_type_for_module_unknown_struct() {
        let module_structs = HashMap::new();
        let result = qualify_type_for_module(
            JuliaType::Struct("Foo".to_string()),
            Some(&"MyModule".to_string()),
            &module_structs,
        );
        // Not found in module, returned unchanged
        assert_eq!(result, JuliaType::Struct("Foo".to_string()));
    }

    #[test]
    fn test_qualify_type_for_module_no_module_path() {
        let module_structs = HashMap::new();
        let result =
            qualify_type_for_module(JuliaType::Struct("Foo".to_string()), None, &module_structs);
        assert_eq!(result, JuliaType::Struct("Foo".to_string()));
    }

    #[test]
    fn test_qualify_type_for_module_parametric_struct() {
        let mut module_structs = HashMap::new();
        let mut mod_structs = HashSet::new();
        mod_structs.insert("Point".to_string());
        module_structs.insert("Geometry".to_string(), mod_structs);

        // "Point{Int64}" should match base name "Point"
        let result = qualify_type_for_module(
            JuliaType::Struct("Point{Int64}".to_string()),
            Some(&"Geometry".to_string()),
            &module_structs,
        );
        assert_eq!(
            result,
            JuliaType::Struct("Geometry.Point{Int64}".to_string())
        );
    }

    #[test]
    fn test_qualify_type_for_module_typeof_inner_struct_issue_7247_8410() {
        let mut module_structs = HashMap::new();
        let mut mod_structs = HashSet::new();
        mod_structs.insert("Foo".to_string());
        module_structs.insert("D7247".to_string(), mod_structs);

        let result = qualify_type_for_module(
            JuliaType::TypeOf(Box::new(JuliaType::Struct("Foo".to_string()))),
            Some(&"D7247".to_string()),
            &module_structs,
        );
        assert_eq!(
            result,
            JuliaType::TypeOf(Box::new(JuliaType::Struct("D7247.Foo".to_string())))
        );
    }

    #[test]
    fn test_qualify_type_non_struct_unchanged() {
        let module_structs = HashMap::new();
        let result =
            qualify_type_for_module(JuliaType::Int64, Some(&"Mod".to_string()), &module_structs);
        assert_eq!(result, JuliaType::Int64);
    }

    // === resolve_abstract_type ===

    #[test]
    fn test_resolve_abstract_type_known() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert("Number".to_string(), None);
        abstract_types.insert("Real".to_string(), Some("Number".to_string()));

        let result = resolve_abstract_type(JuliaType::Struct("Real".to_string()), &abstract_types);
        assert_eq!(
            result,
            JuliaType::AbstractUser("Real".to_string(), Some("Number".to_string()))
        );
    }

    #[test]
    fn test_resolve_abstract_type_unknown() {
        let abstract_types = HashMap::new();
        let result =
            resolve_abstract_type(JuliaType::Struct("MyStruct".to_string()), &abstract_types);
        // Not an abstract type, returned unchanged
        assert_eq!(result, JuliaType::Struct("MyStruct".to_string()));
    }

    #[test]
    fn test_resolve_abstract_type_non_struct_unchanged() {
        let abstract_types = HashMap::new();
        let result = resolve_abstract_type(JuliaType::Float64, &abstract_types);
        assert_eq!(result, JuliaType::Float64);
    }

    #[test]
    fn test_resolve_abstract_type_no_parent() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert("Any".to_string(), None);

        let result = resolve_abstract_type(JuliaType::Struct("Any".to_string()), &abstract_types);
        assert_eq!(result, JuliaType::AbstractUser("Any".to_string(), None));
    }

    #[test]
    fn test_resolve_abstract_type_preserves_concrete_parametric_abstract_issue_9472() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert("Container".to_string(), Some("Any".to_string()));

        let result = resolve_abstract_type(
            JuliaType::Struct("Container{Int64}".to_string()),
            &abstract_types,
        );
        assert_eq!(
            result,
            JuliaType::AbstractUser("Container{Int64}".to_string(), Some("Any".to_string()))
        );
    }

    #[test]
    fn test_resolve_abstract_type_drops_pure_typevars_issue_9472() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert("Container".to_string(), Some("Any".to_string()));

        let result = resolve_abstract_type(
            JuliaType::Struct("Container{T}".to_string()),
            &abstract_types,
        );
        assert_eq!(
            result,
            JuliaType::AbstractUser("Container".to_string(), Some("Any".to_string()))
        );
    }

    #[test]
    fn test_resolve_abstract_type_preserves_parametric_abstract_vector_issue_6239() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert(
            "AbstractVector".to_string(),
            Some("AbstractArray".to_string()),
        );

        let ty = JuliaType::Struct("AbstractVector{T}".to_string());
        let result = resolve_abstract_type(ty.clone(), &abstract_types);
        assert_eq!(result, ty);
    }

    #[test]
    fn test_resolve_abstract_type_preserves_parametric_abstract_matrix_issue_6240() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert(
            "AbstractMatrix".to_string(),
            Some("AbstractArray".to_string()),
        );

        let ty = JuliaType::Struct("AbstractMatrix{T}".to_string());
        let result = resolve_abstract_type(ty.clone(), &abstract_types);
        assert_eq!(result, ty);
    }

    #[test]
    fn test_resolve_abstract_type_preserves_parametric_abstract_array_rank_issue_6243() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert("AbstractArray".to_string(), Some("Any".to_string()));

        let ty = JuliaType::Struct("AbstractArray{T,2}".to_string());
        let result = resolve_abstract_type(ty.clone(), &abstract_types);
        assert_eq!(result, ty);
    }

    #[test]
    fn test_resolve_abstract_type_preserves_parametric_abstract_array_rank1_issue_6245() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert("AbstractArray".to_string(), Some("Any".to_string()));

        let ty = JuliaType::Struct("AbstractArray{T,1}".to_string());
        let result = resolve_abstract_type(ty.clone(), &abstract_types);
        assert_eq!(result, ty);
    }

    #[test]
    fn test_resolve_abstract_type_preserves_parametric_abstract_array_rank_omitted_issue_6247() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert("AbstractArray".to_string(), Some("Any".to_string()));

        let ty = JuliaType::Struct("AbstractArray{T}".to_string());
        let result = resolve_abstract_type(ty.clone(), &abstract_types);
        assert_eq!(result, ty);
    }

    #[test]
    fn test_resolve_abstract_type_preserves_parametric_abstract_array_rank_typevar_issue_6249() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert("AbstractArray".to_string(), Some("Any".to_string()));

        let ty = JuliaType::Struct("AbstractArray{T,N}".to_string());
        let result = resolve_abstract_type(ty.clone(), &abstract_types);
        assert_eq!(result, ty);
    }

    #[test]
    fn test_resolve_abstract_type_preserves_parametric_abstract_range_issue_10150() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert("AbstractRange".to_string(), Some("Any".to_string()));

        let ty = JuliaType::Struct("AbstractRange{T}".to_string());
        let result = resolve_abstract_type(ty.clone(), &abstract_types);
        assert_eq!(result, ty);
    }

    // === resolve_type_alias ===

    #[test]
    fn test_resolve_type_alias_known() {
        let mut aliases = HashMap::new();
        aliases.insert("IntWrapper".to_string(), "Wrapper{Int64}".to_string());

        let result = resolve_type_alias(JuliaType::Struct("IntWrapper".to_string()), &aliases);
        assert_eq!(result, JuliaType::Struct("Wrapper{Int64}".to_string()));
    }

    #[test]
    fn test_resolve_type_alias_unknown() {
        let aliases = HashMap::new();
        let result = resolve_type_alias(JuliaType::Struct("MyType".to_string()), &aliases);
        assert_eq!(result, JuliaType::Struct("MyType".to_string()));
    }

    #[test]
    fn test_resolve_type_alias_non_struct_unchanged() {
        let mut aliases = HashMap::new();
        aliases.insert("Int64".to_string(), "Int32".to_string());
        // JuliaType::Int64 is not a Struct variant, so alias lookup won't apply
        let result = resolve_type_alias(JuliaType::Int64, &aliases);
        assert_eq!(result, JuliaType::Int64);
    }

    // === collect_module_body_binding_names ===

    fn dummy_span() -> crate::span::Span {
        crate::span::Span::new(0, 0, 0, 0, 0, 0)
    }

    fn assign(name: &str) -> Stmt {
        Stmt::Assign {
            var: name.to_string(),
            value: Expr::Literal(Literal::Int(1), dummy_span()),
            span: dummy_span(),
        }
    }

    fn block(stmts: Vec<Stmt>) -> Block {
        Block {
            stmts,
            span: dummy_span(),
        }
    }

    fn collected_helper_function(name: &str) -> Function {
        Function {
            name: name.to_string(),
            params: vec![crate::ir::core::TypedParam::untyped(
                "x".to_string(),
                dummy_span(),
            )],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: block(vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string().into(), dummy_span())),
                span: dummy_span(),
            }]),
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy_span(),
            new_struct_name: None,
        }
    }

    fn nested_helper_let(name: &str) -> Expr {
        Expr::LetBlock {
            bindings: vec![],
            body: block(vec![
                Stmt::FunctionDef {
                    func: Box::new(collected_helper_function(name)),
                    span: dummy_span(),
                },
                Stmt::Expr {
                    expr: Expr::Literal(Literal::Int(1), dummy_span()),
                    span: dummy_span(),
                },
            ]),
            span: dummy_span(),
        }
    }

    #[test]
    fn test_collect_module_body_recurses_into_assignment_expr_letblocks_issue_10227() {
        let helper_name = "__gen_body_10227";
        let body = block(vec![Stmt::Assign {
            var: "s".to_string(),
            value: Expr::Call {
                function: "sum".to_string().into(),
                args: vec![nested_helper_let(helper_name)],
                kwargs: vec![],
                splat_mask: vec![false],
                kwargs_splat_mask: vec![],
                span: dummy_span(),
            },
            span: dummy_span(),
        }]);

        let mut functions = Vec::new();
        let mut module_scope_overrides = HashMap::new();
        collect_module_body_let_functions(
            &body,
            "Gen",
            &mut functions,
            &mut module_scope_overrides,
        );

        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].0.name, helper_name);
        assert_eq!(functions[0].1, None);
        // Issue #10214/#10236: keyed by collection index, not bare name.
        assert_eq!(
            module_scope_overrides.get(&0).map(String::as_str),
            Some("Gen")
        );
    }

    #[test]
    fn test_collect_module_body_uses_exhaustive_stmt_visitor_issue_10346() {
        let body = block(vec![
            Stmt::AddAssign {
                var: "total".to_string(),
                value: nested_helper_let("__gen_add_rhs_10346"),
                span: dummy_span(),
            },
            Stmt::If {
                condition: nested_helper_let("__gen_if_condition_10346"),
                then_branch: block(vec![
                    // A genuine branch definition must not be reclassified as
                    // a lexically scoped synthetic helper root.
                    Stmt::FunctionDef {
                        func: Box::new(collected_helper_function("branch_generic_10346")),
                        span: dummy_span(),
                    },
                    Stmt::Assign {
                        var: "branch_value".to_string(),
                        value: nested_helper_let("__gen_if_branch_rhs_10346"),
                        span: dummy_span(),
                    },
                ]),
                else_branch: None,
                span: dummy_span(),
            },
            Stmt::IndexAssign {
                array: "xs".to_string(),
                indices: vec![nested_helper_let("__gen_index_10346")],
                value: Expr::Literal(Literal::Int(1), dummy_span()),
                span: dummy_span(),
            },
            Stmt::TestThrows {
                exception_type: "ErrorException".to_string(),
                expr: Box::new(nested_helper_let("__gen_test_throws_10346")),
                span: dummy_span(),
            },
        ]);

        let mut functions = Vec::new();
        let mut module_scope_overrides = HashMap::new();
        collect_module_body_let_functions(
            &body,
            "Gen",
            &mut functions,
            &mut module_scope_overrides,
        );

        assert_eq!(
            functions
                .iter()
                .map(|(function, _)| function.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "__gen_add_rhs_10346",
                "__gen_if_condition_10346",
                "__gen_if_branch_rhs_10346",
                "__gen_index_10346",
                "__gen_test_throws_10346",
            ]
        );
        assert_eq!(module_scope_overrides.len(), functions.len());
        assert!(module_scope_overrides
            .values()
            .all(|module_path| module_path == "Gen"));
    }

    /// Module top-level `if`/`elseif`/`else` branches introduce no new scope, so
    /// their assignments are collected as module bindings (Issue #7917).
    #[test]
    fn test_collect_module_bindings_recurses_into_if_branches_issue_7917() {
        // module body:
        //   if true; const x = 1; elseif ...; const y = 1; else; const z = 1; end
        let else_chain = Stmt::If {
            condition: Expr::Literal(Literal::Bool(true), dummy_span()),
            then_branch: block(vec![assign("y")]),
            else_branch: Some(block(vec![assign("z")])),
            span: dummy_span(),
        };
        let body = block(vec![Stmt::If {
            condition: Expr::Literal(Literal::Bool(true), dummy_span()),
            then_branch: block(vec![assign("x")]),
            else_branch: Some(block(vec![else_chain])),
            span: dummy_span(),
        }]);

        let mut names = HashSet::new();
        collect_module_body_binding_names(&body, &mut names);

        assert!(names.contains("x"));
        assert!(names.contains("y"));
        assert!(names.contains("z"));
    }

    /// `for`/`while`/`let`/function bodies DO introduce a local scope at module
    /// top level, so their assignments must NOT leak as module bindings.
    #[test]
    fn test_collect_module_bindings_does_not_leak_loop_scope_issue_7917() {
        let body = block(vec![
            assign("kept"),
            Stmt::For {
                var: "i".to_string(),
                start: Expr::Literal(Literal::Int(1), dummy_span()),
                end: Expr::Literal(Literal::Int(1), dummy_span()),
                step: None,
                body: block(vec![assign("leaked")]),
                span: dummy_span(),
            },
        ]);

        let mut names = HashSet::new();
        collect_module_body_binding_names(&body, &mut names);

        assert!(names.contains("kept"));
        assert!(!names.contains("leaked"));
    }
}
