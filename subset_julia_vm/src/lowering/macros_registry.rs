use crate::ir::core::Block;
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

/// Stored macro definition for expansion during lowering.
#[derive(Debug, Clone)]
pub struct StoredMacroDef {
    pub params: Vec<String>,
    pub param_types: Vec<MacroParamType>,
    pub has_varargs: bool,
    pub body: Block,
    pub span: Span,
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

pub(crate) fn check_type_compatibility(
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
