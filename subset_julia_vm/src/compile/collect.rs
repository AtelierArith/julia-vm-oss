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

fn collect_module_body_binding_names(block: &Block, names: &mut HashSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Assign { var, .. } => {
                names.insert(var.clone());
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
            names.insert(var.clone());
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
    for stmt in &block.stmts {
        collect_stmt_functions(stmt, functions, parent_func_name);
    }
}

pub(in crate::compile) fn collect_expr_functions(
    expr: &Expr,
    functions: &mut Vec<(Function, Option<String>)>,
    parent_func_name: Option<&str>,
) {
    match expr {
        Expr::LetBlock { body, .. } => {
            collect_block_functions(body, functions, parent_func_name);
        }
        Expr::Call { args, kwargs, .. } => {
            for arg in args {
                collect_expr_functions(arg, functions, parent_func_name);
            }
            for (_, value) in kwargs {
                collect_expr_functions(value, functions, parent_func_name);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                collect_expr_functions(arg, functions, parent_func_name);
            }
        }
        Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_expr_functions(arg, functions, parent_func_name);
            }
            for (_, value) in kwargs {
                collect_expr_functions(value, functions, parent_func_name);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_expr_functions(left, functions, parent_func_name);
            collect_expr_functions(right, functions, parent_func_name);
        }
        Expr::UnaryOp { operand, .. } => {
            collect_expr_functions(operand, functions, parent_func_name);
        }
        Expr::Index { array, indices, .. } => {
            collect_expr_functions(array, functions, parent_func_name);
            for index in indices {
                collect_expr_functions(index, functions, parent_func_name);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_expr_functions(start, functions, parent_func_name);
            if let Some(step) = step {
                collect_expr_functions(step, functions, parent_func_name);
            }
            collect_expr_functions(stop, functions, parent_func_name);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            collect_expr_functions(body, functions, parent_func_name);
            collect_expr_functions(iter, functions, parent_func_name);
            if let Some(filter) = filter {
                collect_expr_functions(filter, functions, parent_func_name);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            collect_expr_functions(body, functions, parent_func_name);
            for (_, iter) in iterations {
                collect_expr_functions(iter, functions, parent_func_name);
            }
            if let Some(filter) = filter {
                collect_expr_functions(filter, functions, parent_func_name);
            }
        }
        Expr::FieldAccess { object, .. } => {
            collect_expr_functions(object, functions, parent_func_name);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_expr_functions(condition, functions, parent_func_name);
            collect_expr_functions(then_expr, functions, parent_func_name);
            collect_expr_functions(else_expr, functions, parent_func_name);
        }
        Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
            for elem in elements {
                collect_expr_functions(elem, functions, parent_func_name);
            }
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_expr_functions(value, functions, parent_func_name);
            }
        }
        Expr::Pair { key, value, .. } => {
            collect_expr_functions(key, functions, parent_func_name);
            collect_expr_functions(value, functions, parent_func_name);
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_expr_functions(key, functions, parent_func_name);
                collect_expr_functions(value, functions, parent_func_name);
            }
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                collect_expr_functions(part, functions, parent_func_name);
            }
        }
        Expr::New { args, .. } => {
            for arg in args {
                collect_expr_functions(arg, functions, parent_func_name);
            }
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                collect_expr_functions(base_expr, functions, parent_func_name);
            }
            for arg in type_args {
                collect_expr_functions(arg, functions, parent_func_name);
            }
        }
        Expr::QuoteLiteral { constructor, .. } => {
            collect_expr_functions(constructor, functions, parent_func_name);
        }
        Expr::AssignExpr { value, .. }
        | Expr::ReturnExpr {
            value: Some(value), ..
        } => {
            collect_expr_functions(value, functions, parent_func_name);
        }
        _ => {}
    }
}

pub(in crate::compile) fn collect_stmt_functions(
    stmt: &Stmt,
    functions: &mut Vec<(Function, Option<String>)>,
    parent_func_name: Option<&str>,
) {
    match stmt {
        Stmt::FunctionDef { func, .. } => {
            functions.push((
                (*func.clone()).clone(),
                parent_func_name.map(|s| s.to_string()),
            ));
            // Issue #1744: Recursively collect nested functions from this function's body
            // For 3+ levels of nesting, use qualified name as new parent
            let qualified_parent = if let Some(parent) = parent_func_name {
                format!("{}#{}", parent, func.name)
            } else {
                func.name.clone()
            };
            collect_block_functions(&func.body, functions, Some(&qualified_parent));
        }
        Stmt::EvalFunctionDef { func, .. } => {
            functions.push(((*func.clone()).clone(), None));
            collect_block_functions(&func.body, functions, Some(&func.name));
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            collect_expr_functions(start, functions, parent_func_name);
            collect_expr_functions(end, functions, parent_func_name);
            if let Some(step) = step {
                collect_expr_functions(step, functions, parent_func_name);
            }
            collect_block_functions(body, functions, parent_func_name);
        }
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            collect_expr_functions(iterable, functions, parent_func_name);
            collect_block_functions(body, functions, parent_func_name);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_expr_functions(condition, functions, parent_func_name);
            collect_block_functions(body, functions, parent_func_name);
        }
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            collect_block_functions(body, functions, parent_func_name);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_expr_functions(condition, functions, parent_func_name);
            collect_block_functions(then_branch, functions, parent_func_name);
            if let Some(else_block) = else_branch {
                collect_block_functions(else_block, functions, parent_func_name);
            }
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_block_functions(try_block, functions, parent_func_name);
            if let Some(block) = catch_block {
                collect_block_functions(block, functions, parent_func_name);
            }
            if let Some(block) = else_block {
                collect_block_functions(block, functions, parent_func_name);
            }
            if let Some(block) = finally_block {
                collect_block_functions(block, functions, parent_func_name);
            }
        }
        Stmt::Block(block) => {
            collect_block_functions(block, functions, parent_func_name);
        }
        // Also check expressions for LetBlock (from macro-expanded begin blocks)
        Stmt::Expr { expr, .. } => {
            collect_expr_functions(expr, functions, parent_func_name);
        }
        Stmt::Assign { value, .. } => {
            collect_expr_functions(value, functions, parent_func_name);
        }
        // Index/field/dict assignments also carry value (and index/key) expressions
        // that may embed lambdas. Without recursing here, a lambda in the RHS of
        // `xs[i] = map(x -> ..., xs[i])` (or its index/field/dict-key variants) is
        // compiled as a function value but its generated function is never
        // registered, failing at runtime with `Function '...__lambda_nested_...'
        // not found` (Issue #7615). Mirror the AOT call-graph traversal.
        Stmt::AddAssign { value, .. } => {
            collect_expr_functions(value, functions, parent_func_name);
        }
        Stmt::DictAssign { key, value, .. } => {
            collect_expr_functions(key, functions, parent_func_name);
            collect_expr_functions(value, functions, parent_func_name);
        }
        Stmt::IndexAssign { indices, value, .. } => {
            for index in indices {
                collect_expr_functions(index, functions, parent_func_name);
            }
            collect_expr_functions(value, functions, parent_func_name);
        }
        Stmt::FieldAssign { value, .. } | Stmt::DestructuringAssign { value, .. } => {
            collect_expr_functions(value, functions, parent_func_name);
        }
        // Recurse into return values so that FunctionDefs embedded in LetBlocks inside
        // return statements are discovered (e.g. partial-apply lambdas: Issue #3119).
        Stmt::Return {
            value: Some(expr), ..
        } => {
            collect_expr_functions(expr, functions, parent_func_name);
        }
        Stmt::Test { condition, .. } => {
            collect_expr_functions(condition, functions, parent_func_name);
        }
        Stmt::TestThrows { expr, .. } => {
            collect_expr_functions(expr, functions, parent_func_name);
        }
        _ => {}
    }
}

/// Recursively collect functions from module function bodies.
pub(in crate::compile) fn collect_from_module(
    module: &crate::ir::core::Module,
    inline_functions: &mut Vec<(Function, Option<String>)>,
) {
    for func in &module.functions {
        collect_block_functions(&func.body, inline_functions, Some(&func.name));
    }
    for submodule in &module.submodules {
        collect_from_module(submodule, inline_functions);
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
                "AbstractArray" | "AbstractVector" | "AbstractMatrix"
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
            // An abstract supertype parameterized by INTEGER/BOOL VALUE
            // parameters (`AbsM{2,2,T}`, `StaticMatrix{2,2,T}`) must keep those
            // parameters in the carried name so dispatch can distinguish the
            // `{2,2,T}` and `{3,3,T}` specializations when the argument is a
            // concrete subtype (`ConM{2,2,Float64}`). The historical projection
            // dropped every parameter to the bare family name, collapsing all
            // value-parameter specializations into one signature so the
            // last-defined one always won (Issue #7960). Only retain the
            // parameters when at least one is a value literal: type-only
            // parametric abstracts (`AbstractDict{K,V}`) keep the bare-family
            // representation the rest of the dispatcher already handles.
            let stored_name = match name.find('{') {
                Some(open) if name_has_value_param(&name[open..]) => {
                    format!("{lookup_name}{}", &name[open..])
                }
                _ => lookup_name.to_string(),
            };
            return JuliaType::AbstractUser(stored_name, parent.clone());
        }
    }
    jt
}

/// Whether a `{...}` parameter list spells at least one integer/bool VALUE
/// parameter (e.g. the `2`s in `{2,2,T}`). Used to decide whether a parametric
/// abstract supertype must keep its parameters in the carried dispatch name
/// (Issue #7960). Type-only parameter lists (`{K,V}`, `{T<:Real}`) return false.
fn name_has_value_param(params_suffix: &str) -> bool {
    let inner = params_suffix
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .unwrap_or(params_suffix);
    split_top_level_type_args(inner).into_iter().any(|tok| {
        let tok = tok.trim();
        tok.parse::<i128>().is_ok() || tok == "true" || tok == "false"
    })
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
