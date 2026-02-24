//! Variant naming helpers for diagnostic messages.

use crate::ir::core::{Expr, Stmt};

/// Return the variant name of a `Stmt` for diagnostic messages.
pub(super) fn stmt_variant_name(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Block(_) => "Block",
        Stmt::Assign { .. } => "Assign",
        Stmt::AddAssign { .. } => "AddAssign",
        Stmt::For { .. } => "For",
        Stmt::ForEach { .. } => "ForEach",
        Stmt::ForEachTuple { .. } => "ForEachTuple",
        Stmt::While { .. } => "While",
        Stmt::If { .. } => "If",
        Stmt::Break { .. } => "Break",
        Stmt::Continue { .. } => "Continue",
        Stmt::Try { .. } => "Try",
        Stmt::Return { .. } => "Return",
        Stmt::Expr { .. } => "Expr",
        Stmt::Timed { .. } => "Timed",
        Stmt::Test { .. } => "Test",
        Stmt::TestSet { .. } => "TestSet",
        Stmt::TestThrows { .. } => "TestThrows",
        Stmt::IndexAssign { .. } => "IndexAssign",
        Stmt::FieldAssign { .. } => "FieldAssign",
        Stmt::DestructuringAssign { .. } => "DestructuringAssign",
        Stmt::DictAssign { .. } => "DictAssign",
        Stmt::Using { .. } => "Using",
        Stmt::Export { .. } => "Export",
        Stmt::FunctionDef { .. } => "FunctionDef",
        Stmt::Label { .. } => "Label",
        Stmt::Goto { .. } => "Goto",
        Stmt::EnumDef { .. } => "EnumDef",
    }
}

/// Return the variant name of an `Expr` for diagnostic messages.
pub(super) fn expr_variant_name(expr: &Expr) -> &'static str {
    match expr {
        Expr::Literal(..) => "Literal",
        Expr::Var(..) => "Var",
        Expr::BinaryOp { .. } => "BinaryOp",
        Expr::UnaryOp { .. } => "UnaryOp",
        Expr::Call { .. } => "Call",
        Expr::Builtin { .. } => "Builtin",
        Expr::ArrayLiteral { .. } => "ArrayLiteral",
        Expr::TypedEmptyArray { .. } => "TypedEmptyArray",
        Expr::Index { .. } => "Index",
        Expr::Range { .. } => "Range",
        Expr::Comprehension { .. } => "Comprehension",
        Expr::MultiComprehension { .. } => "MultiComprehension",
        Expr::Generator { .. } => "Generator",
        Expr::SliceAll { .. } => "SliceAll",
        Expr::FieldAccess { .. } => "FieldAccess",
        Expr::FunctionRef { .. } => "FunctionRef",
        Expr::TupleLiteral { .. } => "TupleLiteral",
        Expr::NamedTupleLiteral { .. } => "NamedTupleLiteral",
        Expr::Pair { .. } => "Pair",
        Expr::DictLiteral { .. } => "DictLiteral",
        Expr::LetBlock { .. } => "LetBlock",
        Expr::StringConcat { .. } => "StringConcat",
        Expr::ModuleCall { .. } => "ModuleCall",
        Expr::Ternary { .. } => "Ternary",
        Expr::New { .. } => "New",
        Expr::DynamicTypeConstruct { .. } => "DynamicTypeConstruct",
        Expr::QuoteLiteral { .. } => "QuoteLiteral",
        Expr::AssignExpr { .. } => "AssignExpr",
        Expr::ReturnExpr { .. } => "ReturnExpr",
        Expr::BreakExpr { .. } => "BreakExpr",
        Expr::ContinueExpr { .. } => "ContinueExpr",
    }
}
