//! Collection and resolution helpers for the compilation driver.
//!
//! These functions collect structs, functions, module info, using imports,
//! and struct literal types from the IR tree. They also handle type qualification
//! and resolution for module-scoped types.

use crate::ir::core::{Block, Expr, Function, Literal, Stmt, UsingImport};
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

    // Collect function names
    let func_names: HashSet<String> = module.functions.iter().map(|f| f.name.clone()).collect();
    module_functions.insert(module_path.clone(), func_names);

    // Collect exports
    let export_names: HashSet<String> = module.exports.iter().cloned().collect();
    module_exports.insert(module_path.clone(), export_names);

    // Collect constants from module body (top-level assignments)
    let mut const_names: HashSet<String> = HashSet::new();
    for stmt in &module.body.stmts {
        if let crate::ir::core::Stmt::Assign { var, .. } = stmt {
            const_names.insert(var.clone());
        }
    }
    module_constants.insert(module_path.clone(), const_names);

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
        Expr::Call { args, .. } => {
            for arg in args {
                collect_expr_functions(arg, functions, parent_func_name);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_expr_functions(left, functions, parent_func_name);
            collect_expr_functions(right, functions, parent_func_name);
        }
        Expr::UnaryOp { operand, .. } => {
            collect_expr_functions(operand, functions, parent_func_name);
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
        Stmt::For { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachTuple { body, .. }
        | Stmt::While { body, .. }
        | Stmt::Timed { body, .. }
        | Stmt::TestSet { body, .. } => {
            collect_block_functions(body, functions, parent_func_name);
        }
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
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
        // Recurse into return values so that FunctionDefs embedded in LetBlocks inside
        // return statements are discovered (e.g. partial-apply lambdas: Issue #3119).
        Stmt::Return { value: Some(expr), .. } => {
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
            Stmt::Expr { expr, .. } => {
                collect_struct_literal_types_from_expr(expr, struct_names)
            }
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
        // Recursively qualify element types in VectorOf
        (JuliaType::VectorOf(elem), _) => {
            let qualified_elem = qualify_type_for_module(
                elem.as_ref().clone(),
                module_path,
                module_struct_names,
            );
            JuliaType::VectorOf(Box::new(qualified_elem))
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
        if let Some(parent) = abstract_type_parents.get(base_name) {
            // This is an abstract type - convert to AbstractUser
            return JuliaType::AbstractUser(base_name.to_string(), parent.clone());
        }
    }
    jt
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
        let result = qualify_type_for_module(
            JuliaType::Struct("Foo".to_string()),
            None,
            &module_structs,
        );
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
    fn test_qualify_type_non_struct_unchanged() {
        let module_structs = HashMap::new();
        let result = qualify_type_for_module(
            JuliaType::Int64,
            Some(&"Mod".to_string()),
            &module_structs,
        );
        assert_eq!(result, JuliaType::Int64);
    }

    // === resolve_abstract_type ===

    #[test]
    fn test_resolve_abstract_type_known() {
        let mut abstract_types = HashMap::new();
        abstract_types.insert("Number".to_string(), None);
        abstract_types.insert("Real".to_string(), Some("Number".to_string()));

        let result =
            resolve_abstract_type(JuliaType::Struct("Real".to_string()), &abstract_types);
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

        let result =
            resolve_abstract_type(JuliaType::Struct("Any".to_string()), &abstract_types);
        assert_eq!(
            result,
            JuliaType::AbstractUser("Any".to_string(), None)
        );
    }

    // === resolve_type_alias ===

    #[test]
    fn test_resolve_type_alias_known() {
        let mut aliases = HashMap::new();
        aliases.insert("IntWrapper".to_string(), "Wrapper{Int64}".to_string());

        let result =
            resolve_type_alias(JuliaType::Struct("IntWrapper".to_string()), &aliases);
        assert_eq!(result, JuliaType::Struct("Wrapper{Int64}".to_string()));
    }

    #[test]
    fn test_resolve_type_alias_unknown() {
        let aliases = HashMap::new();
        let result =
            resolve_type_alias(JuliaType::Struct("MyType".to_string()), &aliases);
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
}
