//! Canonical Expr head registry for metaprogramming paths.
//!
//! Julia `Expr` heads cross three sjulia subsystems: quote construction,
//! macro-return lowering, and runtime `eval`. Keep the symbolic names and
//! per-path coverage in one table so unsupported directions are visible when a
//! new head is added.

use crate::vm::value::ExprValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ExprHead {
    Escape,
    HygienicScope,
    Block,
    Function,
    Struct,
    Assign,
    Const,
    Export,
    Public,
    AddAssign,
    Call,
    For,
    While,
    If,
    ElseIf,
    Let,
    Local,
    String,
    TypeAssert,
    Where,
    Subtype,
    Tuple,
    Vect,
    Row,
    Hcat,
    Vcat,
    Ref,
    MacroCall,
    Generator,
    Comprehension,
    Arrow,
    Curly,
    ParametrizedTypeExpression,
    Interpolation,
    Splat,
    Adjoint,
    Return,
    Quote,
    Dot,
    Try,
    SymbolicLabel,
    SymbolicGoto,
    Comparison,
    AndAnd,
    OrOr,
    CopyAst,
    Parameters,
    Kw,
    Meta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExprHeadSpec {
    pub head: ExprHead,
    pub cst_to_expr_value: bool,
    pub macro_return_to_stmt: bool,
    pub macro_return_to_expr: bool,
    pub runtime_eval: bool,
}

impl ExprHead {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Escape => "escape",
            Self::HygienicScope => "hygienic-scope",
            Self::Block => "block",
            Self::Function => "function",
            Self::Struct => "struct",
            Self::Assign => "=",
            Self::Const => "const",
            Self::Export => "export",
            Self::Public => "public",
            Self::AddAssign => "+=",
            Self::Call => "call",
            Self::For => "for",
            Self::While => "while",
            Self::If => "if",
            Self::ElseIf => "elseif",
            Self::Let => "let",
            Self::Local => "local",
            Self::String => "string",
            Self::TypeAssert => "::",
            Self::Where => "where",
            Self::Subtype => "<:",
            Self::Tuple => "tuple",
            Self::Vect => "vect",
            Self::Row => "row",
            Self::Hcat => "hcat",
            Self::Vcat => "vcat",
            Self::Ref => "ref",
            Self::MacroCall => "macrocall",
            Self::Generator => "generator",
            Self::Comprehension => "comprehension",
            Self::Arrow => "->",
            Self::Curly => "curly",
            Self::ParametrizedTypeExpression => "parametrizedtypeexpression",
            Self::Interpolation => "$",
            Self::Splat => "...",
            Self::Adjoint => "'",
            Self::Return => "return",
            Self::Quote => "quote",
            Self::Dot => ".",
            Self::Try => "try",
            Self::SymbolicLabel => "symboliclabel",
            Self::SymbolicGoto => "symbolicgoto",
            Self::Comparison => "comparison",
            Self::AndAnd => "&&",
            Self::OrOr => "||",
            Self::CopyAst => "copyast",
            Self::Parameters => "parameters",
            Self::Kw => "kw",
            Self::Meta => "meta",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "escape" => Self::Escape,
            "hygienic-scope" => Self::HygienicScope,
            "block" => Self::Block,
            "function" => Self::Function,
            "struct" => Self::Struct,
            "=" => Self::Assign,
            "const" => Self::Const,
            "export" => Self::Export,
            "public" => Self::Public,
            "+=" => Self::AddAssign,
            "call" => Self::Call,
            "for" => Self::For,
            "while" => Self::While,
            "if" => Self::If,
            "elseif" => Self::ElseIf,
            "let" => Self::Let,
            "local" => Self::Local,
            "string" => Self::String,
            "::" => Self::TypeAssert,
            "where" => Self::Where,
            "<:" => Self::Subtype,
            "tuple" => Self::Tuple,
            "vect" => Self::Vect,
            "row" => Self::Row,
            "hcat" => Self::Hcat,
            "vcat" => Self::Vcat,
            "ref" => Self::Ref,
            "macrocall" => Self::MacroCall,
            "generator" => Self::Generator,
            "comprehension" => Self::Comprehension,
            "->" => Self::Arrow,
            "curly" => Self::Curly,
            "parametrizedtypeexpression" => Self::ParametrizedTypeExpression,
            "$" => Self::Interpolation,
            "..." => Self::Splat,
            "'" => Self::Adjoint,
            "return" => Self::Return,
            "quote" => Self::Quote,
            "." => Self::Dot,
            "try" => Self::Try,
            "symboliclabel" => Self::SymbolicLabel,
            "symbolicgoto" => Self::SymbolicGoto,
            "comparison" => Self::Comparison,
            "&&" => Self::AndAnd,
            "||" => Self::OrOr,
            "copyast" => Self::CopyAst,
            "parameters" => Self::Parameters,
            "kw" => Self::Kw,
            "meta" => Self::Meta,
            _ => return None,
        })
    }

    pub(crate) fn from_expr(expr: &ExprValue) -> Option<Self> {
        Self::from_name(expr.head.as_str())
    }

    pub(crate) fn is_expr(expr: &ExprValue, head: Self) -> bool {
        Self::from_expr(expr) == Some(head)
    }

    pub(crate) fn spec(self) -> &'static ExprHeadSpec {
        EXPR_HEAD_REGISTRY
            .iter()
            .find(|spec| spec.head == self)
            .expect("all ExprHead variants must be present in EXPR_HEAD_REGISTRY")
    }
}

pub(crate) const EXPR_HEAD_REGISTRY: &[ExprHeadSpec] = &[
    spec(ExprHead::Escape, true, true, true, false),
    spec(ExprHead::HygienicScope, true, true, true, false),
    spec(ExprHead::Block, true, true, true, true),
    spec(ExprHead::Function, true, true, false, false),
    spec(ExprHead::Struct, true, true, false, false),
    spec(ExprHead::Assign, true, true, true, true),
    spec(ExprHead::Const, true, true, true, false),
    spec(ExprHead::Export, true, true, true, false),
    spec(ExprHead::Public, true, true, true, false),
    spec(ExprHead::AddAssign, true, true, false, false),
    spec(ExprHead::Call, true, true, true, true),
    spec(ExprHead::For, true, true, true, false),
    spec(ExprHead::While, true, false, false, false),
    spec(ExprHead::If, true, true, true, true),
    spec(ExprHead::ElseIf, true, true, true, true),
    spec(ExprHead::Let, true, true, true, true),
    spec(ExprHead::Local, true, true, true, false),
    spec(ExprHead::String, true, false, true, true),
    spec(ExprHead::TypeAssert, true, false, true, false),
    spec(ExprHead::Where, true, false, true, false),
    spec(ExprHead::Subtype, true, false, true, false),
    spec(ExprHead::Tuple, true, false, true, true),
    spec(ExprHead::Vect, true, false, true, true),
    spec(ExprHead::Row, true, false, true, false),
    spec(ExprHead::Hcat, true, false, true, false),
    spec(ExprHead::Vcat, true, false, true, false),
    spec(ExprHead::Ref, true, false, true, true),
    spec(ExprHead::MacroCall, true, false, true, false),
    spec(ExprHead::Generator, true, false, false, false),
    spec(ExprHead::Comprehension, true, false, false, false),
    spec(ExprHead::Arrow, true, false, true, false),
    spec(ExprHead::Curly, true, false, true, true),
    spec(
        ExprHead::ParametrizedTypeExpression,
        false,
        false,
        false,
        true,
    ),
    spec(ExprHead::Interpolation, true, false, true, false),
    spec(ExprHead::Splat, true, false, true, false),
    spec(ExprHead::Adjoint, true, false, true, false),
    spec(ExprHead::Return, true, true, true, true),
    spec(ExprHead::Quote, true, false, true, true),
    spec(ExprHead::Dot, true, false, true, false),
    spec(ExprHead::Try, true, true, true, true),
    spec(ExprHead::SymbolicLabel, true, false, false, false),
    spec(ExprHead::SymbolicGoto, true, false, false, false),
    spec(ExprHead::Comparison, true, false, false, true),
    spec(ExprHead::AndAnd, true, false, false, true),
    spec(ExprHead::OrOr, true, false, false, true),
    spec(ExprHead::CopyAst, true, false, false, true),
    spec(ExprHead::Parameters, true, false, false, false),
    spec(ExprHead::Kw, true, false, false, false),
    spec(ExprHead::Meta, true, true, false, false),
];

const fn spec(
    head: ExprHead,
    cst_to_expr_value: bool,
    macro_return_to_stmt: bool,
    macro_return_to_expr: bool,
    runtime_eval: bool,
) -> ExprHeadSpec {
    ExprHeadSpec {
        head,
        cst_to_expr_value,
        macro_return_to_stmt,
        macro_return_to_expr,
        runtime_eval,
    }
}

#[cfg(test)]
mod tests {
    use super::{ExprHead, EXPR_HEAD_REGISTRY};

    #[test]
    fn expr_head_registry_round_trips_names() {
        for spec in EXPR_HEAD_REGISTRY {
            let name = spec.head.as_str();
            assert_eq!(ExprHead::from_name(name), Some(spec.head));
            assert_eq!(spec.head.spec().head, spec.head);
        }
    }

    #[test]
    fn expr_head_registry_has_unique_names() {
        let mut names = EXPR_HEAD_REGISTRY
            .iter()
            .map(|spec| spec.head.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), EXPR_HEAD_REGISTRY.len());
    }
}
