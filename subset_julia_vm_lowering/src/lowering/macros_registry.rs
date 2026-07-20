use std::collections::HashSet;
use std::sync::OnceLock;

use crate::ir::core::{Block, Function, StructDef};
use crate::lowering::LambdaContext;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::span::Span;

/// Type annotation for macro parameters used in type-based dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MacroParamType {
    #[default]
    Any,
    Symbol,
    Expr,
    Integer,
    Float,
    String,
    LineNumberNode,
}

/// Module metadata needed when expanding a module-defined macro.
#[derive(Debug, Clone)]
pub struct MacroHygieneInfo {
    pub module: String,
    pub members: HashSet<String>,
    pub exports: HashSet<String>,
}

/// Stored macro definition for expansion during lowering.
#[derive(Debug, Clone)]
pub struct StoredMacroDef {
    pub params: Vec<String>,
    pub param_types: Vec<MacroParamType>,
    pub has_varargs: bool,
    pub body: Block,
    pub expansion_functions: Vec<Function>,
    pub expansion_structs: Vec<StructDef>,
    pub hygiene: Option<MacroHygieneInfo>,
    pub span: Span,
}

/// Integration-owned macro registry seam (Issue #8640).
///
/// Lowering-core must not depend on `base_loader` / `stdlib_loader` directly:
/// those modules embed source registries and belong to the integration crate.
/// This trait keeps lowering below compile/vm while preserving the existing
/// registry behavior through an installed implementation.
pub trait MacroRegistry: Sync {
    fn ensure_stdlib_macros_loaded(&self, module_name: &str);
    fn ensure_bundled_package_macros_loaded(&self, module_name: &str);
    fn has_base_macro(&self, name: &str) -> bool;
    fn get_base_macro(&self, name: &str) -> Option<StoredMacroDef>;
    fn get_base_macro_with_arity(&self, name: &str, arity: usize) -> Option<StoredMacroDef>;
    fn has_stdlib_macro(&self, module_name: &str, name: &str) -> bool;
    fn get_stdlib_macro(&self, module_name: &str, name: &str) -> Option<StoredMacroDef>;
    fn has_bundled_package_macro(&self, module_name: &str, name: &str) -> bool;
    fn get_bundled_package_macro(&self, module_name: &str, name: &str) -> Option<StoredMacroDef>;
    fn add_bundled_package_macro_context(&self, module_name: &str, lambda_ctx: &LambdaContext);
}

static MACRO_REGISTRY: OnceLock<&'static dyn MacroRegistry> = OnceLock::new();

pub fn install_macro_registry(registry: &'static dyn MacroRegistry) {
    let _ = MACRO_REGISTRY.set(registry);
}

fn registry() -> Option<&'static dyn MacroRegistry> {
    MACRO_REGISTRY.get().copied()
}

pub fn ensure_stdlib_macros_loaded(module_name: &str) {
    if let Some(registry) = registry() {
        registry.ensure_stdlib_macros_loaded(module_name);
    }
}

pub fn ensure_bundled_package_macros_loaded(module_name: &str) {
    if let Some(registry) = registry() {
        registry.ensure_bundled_package_macros_loaded(module_name);
    }
}

pub fn has_base_macro(name: &str) -> bool {
    registry().is_some_and(|registry| registry.has_base_macro(name))
}

pub fn get_base_macro(name: &str) -> Option<StoredMacroDef> {
    registry().and_then(|registry| registry.get_base_macro(name))
}

pub fn get_base_macro_with_arity(name: &str, arity: usize) -> Option<StoredMacroDef> {
    registry().and_then(|registry| registry.get_base_macro_with_arity(name, arity))
}

pub fn has_stdlib_macro(module_name: &str, name: &str) -> bool {
    registry().is_some_and(|registry| registry.has_stdlib_macro(module_name, name))
}

pub fn get_stdlib_macro(module_name: &str, name: &str) -> Option<StoredMacroDef> {
    registry().and_then(|registry| registry.get_stdlib_macro(module_name, name))
}

pub fn has_bundled_package_macro(module_name: &str, name: &str) -> bool {
    registry().is_some_and(|registry| registry.has_bundled_package_macro(module_name, name))
}

pub fn get_bundled_package_macro(module_name: &str, name: &str) -> Option<StoredMacroDef> {
    registry().and_then(|registry| registry.get_bundled_package_macro(module_name, name))
}

pub fn add_bundled_package_macro_context(module_name: &str, lambda_ctx: &LambdaContext) {
    if let Some(registry) = registry() {
        registry.add_bundled_package_macro_context(module_name, lambda_ctx);
    }
}

/// Determine the MacroParamType of a CST node.
pub fn get_node_macro_type<'a>(walker: &CstWalker<'a>, node: &Node<'a>) -> MacroParamType {
    match walker.kind(node) {
        NodeKind::Identifier => MacroParamType::Symbol,
        NodeKind::IntegerLiteral => MacroParamType::Integer,
        NodeKind::FloatLiteral => MacroParamType::Float,
        NodeKind::StringLiteral | NodeKind::CharacterLiteral => MacroParamType::String,
        _ => MacroParamType::Expr,
    }
}

fn type_matches(param_type: &MacroParamType, arg_type: &MacroParamType) -> bool {
    match param_type {
        MacroParamType::Any => true,
        _ => param_type == arg_type,
    }
}

pub fn check_type_compatibility(
    param_types: &[MacroParamType],
    arg_types: &[MacroParamType],
    has_varargs: bool,
) -> (bool, usize) {
    let mut specificity = 0;
    let params_to_check = if has_varargs && !param_types.is_empty() {
        &param_types[..param_types.len() - 1]
    } else {
        param_types
    };

    for (i, param_type) in params_to_check.iter().enumerate() {
        if i >= arg_types.len() {
            return (false, 0);
        }
        if !type_matches(param_type, &arg_types[i]) {
            return (false, 0);
        }
        if *param_type != MacroParamType::Any {
            specificity += 1;
        }
    }

    (true, specificity)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── check_type_compatibility ──────────────────────────────────────────────

    #[test]
    fn test_check_type_compatibility_empty_params_matches_empty_args() {
        let (ok, specificity) = check_type_compatibility(&[], &[], false);
        assert!(ok);
        assert_eq!(specificity, 0);
    }

    #[test]
    fn test_check_type_compatibility_any_matches_any_arg_type() {
        let params = vec![MacroParamType::Any];
        let args = vec![MacroParamType::Integer];
        let (ok, specificity) = check_type_compatibility(&params, &args, false);
        assert!(ok);
        assert_eq!(specificity, 0, "Any param contributes 0 specificity");
    }

    #[test]
    fn test_check_type_compatibility_exact_type_match_gives_specificity() {
        let params = vec![MacroParamType::Symbol];
        let args = vec![MacroParamType::Symbol];
        let (ok, specificity) = check_type_compatibility(&params, &args, false);
        assert!(ok);
        assert_eq!(specificity, 1, "Exact non-Any match gives specificity 1");
    }

    #[test]
    fn test_check_type_compatibility_type_mismatch_returns_false() {
        let params = vec![MacroParamType::Symbol];
        let args = vec![MacroParamType::Integer];
        let (ok, _) = check_type_compatibility(&params, &args, false);
        assert!(!ok);
    }

    #[test]
    fn test_check_type_compatibility_too_few_args_returns_false() {
        let params = vec![MacroParamType::Symbol, MacroParamType::Integer];
        let args = vec![MacroParamType::Symbol]; // only 1 arg for 2 params
        let (ok, _) = check_type_compatibility(&params, &args, false);
        assert!(!ok);
    }

    #[test]
    fn test_check_type_compatibility_varargs_skips_last_param() {
        // Varargs: last param (Any) is the varargs collector — skipped in check.
        // Provided 1 arg matching first param (Symbol) → ok.
        let params = vec![MacroParamType::Symbol, MacroParamType::Any];
        let args = vec![MacroParamType::Symbol];
        let (ok, specificity) = check_type_compatibility(&params, &args, true);
        assert!(ok);
        assert_eq!(specificity, 1);
    }

    #[test]
    fn test_check_type_compatibility_multiple_specific_params() {
        let params = vec![
            MacroParamType::Symbol,
            MacroParamType::Integer,
            MacroParamType::String,
        ];
        let args = vec![
            MacroParamType::Symbol,
            MacroParamType::Integer,
            MacroParamType::String,
        ];
        let (ok, specificity) = check_type_compatibility(&params, &args, false);
        assert!(ok);
        assert_eq!(specificity, 3);
    }
}
