//! Helper functions for expression lowering.
//!
//! This module contains utility functions for:
//! - Raw string escape processing
//! - Operator classification and mapping
//! - Builtin function name mapping

use crate::ir::core::{BinaryOp, BuiltinOp, Expr, UnaryOp};
use crate::span::Span;

/// Process escape sequences in raw strings.
/// In Julia, raw strings still process \\ (to \) and \" (to ")
/// but all other escape sequences like \n, \t are kept as-is.
pub(crate) fn process_raw_string_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                match next {
                    '\\' => {
                        // \\ -> single backslash
                        result.push('\\');
                        chars.next();
                    }
                    '"' => {
                        // \" -> quote
                        result.push('"');
                        chars.next();
                    }
                    _ => {
                        // Keep the backslash and the next character as-is
                        result.push('\\');
                    }
                }
            } else {
                // Trailing backslash
                result.push('\\');
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Check if an operator should be flattened when chained.
/// Julia flattens associative operators like `+` and `*` so that `a + b + c` becomes `+(a, b, c)`.
pub(crate) fn is_flattenable_operator(op: &str) -> bool {
    matches!(op, "+" | "*")
}

/// Check if an operator is a comparison operator that can be chained.
/// In Julia, `a < b < c` is equivalent to `(a < b) && (b < c)`.
pub(crate) fn is_comparison_operator(op: &str) -> bool {
    matches!(op, "<" | ">" | "<=" | ">=" | "==" | "!=" | "===" | "!==")
}

/// Check if a node kind represents an operator token.
pub(crate) fn is_operator_token(kind: &str) -> bool {
    matches!(
        kind,
        "+" | "-"
            | "*"
            | "/"
            | "^"
            | "%"
            | "\\"
            | "=="
            | "!="
            | "<"
            | ">"
            | "<="
            | ">="
            | "&&"
            | "||"
            | "&"
            | "|"
            | "⊻"
            | "<<"
            | ">>"
            | ">>>"
            | "÷"
            | "∈"
            | "∉"
            | "⊆"
            | "⊇"
            | "⊂"
            | "⊃"
            | "⊈"
            | "⊊"
            | "⊉"
            | "⊋"
            | "∋"
            | "∌"
            | "+="
            | "-="
            | "*="
            | "/="
            | ".+"
            | ".-"
            | ".*"
            | "./"
            | ".^"
            | ".%"
            | ".=="
            | ".!="
            | ".<"
            | ".>"
            | ".<="
            | ".>="
            | "=>"
            | ":"
            | ".."
    )
}

/// Check if an operator is a broadcast operator (starts with `.`).
pub(crate) fn is_broadcast_op(op: &str) -> bool {
    op.starts_with('.') && op.len() > 1
}

/// Strip the base operator from a broadcast operator (e.g., ".+" -> "+").
pub(crate) fn strip_broadcast_dot(op: &str) -> &str {
    if op.starts_with('.') && op.len() > 1 {
        &op[1..]
    } else {
        op
    }
}

/// Build `materialize(Broadcasted(fn_ref, (args...)))` IR for broadcast expressions.
///
/// For fusion support: if an arg is itself `materialize(Broadcasted(...))`,
/// strip the outer `materialize` to keep the inner `Broadcasted` lazy.
/// This enables loop fusion for nested dot expressions like `sin.(x) .+ 1`.
///
/// # Arguments
/// * `fn_name` - The base operator/function name (e.g., "+" not ".+")
/// * `args` - The already-lowered argument expressions
/// * `span` - Source span for error reporting
pub(crate) fn make_broadcasted_call(fn_name: &str, args: Vec<Expr>, span: Span) -> Expr {
    // Strip materialize wrappers from args that are broadcast results (fusion)
    let fused_args: Vec<Expr> = args.into_iter().map(strip_materialize).collect();

    // Build: Broadcasted(fn_ref, (arg1, arg2, ...))
    let fn_ref = Expr::FunctionRef {
        name: fn_name.to_string(),
        span,
    };
    let args_tuple = Expr::TupleLiteral {
        elements: fused_args,
        span,
    };
    let broadcasted_call = Expr::Call {
        function: "Broadcasted".to_string(),
        args: vec![fn_ref, args_tuple],
        kwargs: Vec::new(),
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span,
    };

    // Wrap in materialize: materialize(Broadcasted(...))
    Expr::Call {
        function: "materialize".to_string(),
        args: vec![broadcasted_call],
        kwargs: Vec::new(),
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span,
    }
}

/// If `expr` is `materialize(Broadcasted(...))`, return the inner `Broadcasted(...)` call.
/// Otherwise return the expression unchanged. This enables broadcast fusion.
fn strip_materialize(expr: Expr) -> Expr {
    if let Expr::Call {
        ref function,
        ref args,
        ..
    } = expr
    {
        if function == "materialize" && args.len() == 1 {
            if let Expr::Call {
                function: ref inner_fn,
                ..
            } = args[0]
            {
                if inner_fn == "Broadcasted" {
                    // Return the inner Broadcasted(...) call, stripping materialize
                    return args[0].clone();
                }
            }
        }
    }
    expr
}

/// Map operator string to BinaryOp enum.
pub(crate) fn map_binary_op(op: &str) -> Option<BinaryOp> {
    match op {
        "+" => Some(BinaryOp::Add),
        "-" => Some(BinaryOp::Sub),
        "*" => Some(BinaryOp::Mul),
        "/" => Some(BinaryOp::Div),
        "%" => Some(BinaryOp::Mod),
        "^" => Some(BinaryOp::Pow),
        "<" => Some(BinaryOp::Lt),
        ">" => Some(BinaryOp::Gt),
        "<=" => Some(BinaryOp::Le),
        ">=" => Some(BinaryOp::Ge),
        "==" => Some(BinaryOp::Eq),
        "!=" => Some(BinaryOp::Ne),
        "===" => Some(BinaryOp::Egal),
        "!==" => Some(BinaryOp::NotEgal),
        "<:" => Some(BinaryOp::Subtype),
        "&&" => Some(BinaryOp::And),
        "||" => Some(BinaryOp::Or),
        _ => None,
    }
}

/// Map operator string to UnaryOp enum.
pub(crate) fn map_unary_op(op: &str) -> Option<UnaryOp> {
    Some(match op {
        "-" => UnaryOp::Neg,
        "+" => UnaryOp::Pos,
        "!" => UnaryOp::Not,
        _ => return None,
    })
}

/// Map function name to BuiltinOp if it's a known builtin.
pub(crate) fn map_builtin_name(name: &str) -> Option<BuiltinOp> {
    Some(match name {
        "rand" => BuiltinOp::Rand,
        "sqrt" => BuiltinOp::Sqrt,
        "ifelse" => BuiltinOp::IfElse,
        // Array builtins
        "zeros" => BuiltinOp::Zeros,
        "ones" => BuiltinOp::Ones,
        "reshape" => BuiltinOp::Reshape,
        // Note: trues, falses, fill are now Pure Julia (base/array.jl)
        "length" => BuiltinOp::Length,
        "size" => BuiltinOp::Size,
        "push!" => BuiltinOp::Push,
        "pop!" => BuiltinOp::Pop,
        "zero" => BuiltinOp::Zero,
        // Note: complex, real, imag, conj, abs, abs2, transpose are Pure Julia
        // RNG constructors
        "StableRNG" => BuiltinOp::StableRNG,
        "Xoshiro" => BuiltinOp::XoshiroRNG,
        // Normal distribution
        "randn" => BuiltinOp::Randn,
        // Tuple operations
        "first" => BuiltinOp::TupleFirst,
        "last" => BuiltinOp::TupleLast,
        // Dict operations
        // Note: haskey, get, merge, keys, values, pairs are now Pure Julia (Issue #2572, #2573, #2669)
        "delete!" => BuiltinOp::DictDelete,
        // Broadcasting control
        "Ref" => BuiltinOp::Ref,
        // Type operations
        "typeof" => BuiltinOp::TypeOf,
        "isa" => BuiltinOp::Isa,
        // Iterator Protocol
        "iterate" => BuiltinOp::Iterate,
        "collect" => BuiltinOp::Collect,
        // Macro hygiene
        // Note: gensym is now Pure Julia (meta.jl) - Issue #294
        "esc" => BuiltinOp::Esc,
        // Metaprogramming
        "eval" => BuiltinOp::Eval,
        "macroexpand" => BuiltinOp::MacroExpand,
        "macroexpand!" => BuiltinOp::MacroExpandBang,
        "include_string" => BuiltinOp::IncludeString,
        "evalfile" => BuiltinOp::EvalFile,
        "Symbol" => BuiltinOp::SymbolNew,
        "Expr" => BuiltinOp::ExprNew,
        "LineNumberNode" => BuiltinOp::LineNumberNodeNew,
        "QuoteNode" => BuiltinOp::QuoteNodeNew,
        "GlobalRef" => BuiltinOp::GlobalRefNew,
        // Timing
        "time_ns" => BuiltinOp::TimeNs,
        // Test operations (for Pure Julia @test/@testset/@test_throws macros)
        "_test_record!" => BuiltinOp::TestRecord,
        "_test_record_broken!" => BuiltinOp::TestRecordBroken,
        "_testset_begin!" => BuiltinOp::TestSetBegin,
        "_testset_end!" => BuiltinOp::TestSetEnd,
        // Note: seed! is only available via Random.seed!() (not exported by default)
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── process_raw_string_escapes ────────────────────────────────────────────

    #[test]
    fn test_process_raw_string_escapes_double_backslash_becomes_single() {
        // \\\\ → \\  (two backslashes → one backslash)
        assert_eq!(process_raw_string_escapes("a\\\\b"), "a\\b");
    }

    #[test]
    fn test_process_raw_string_escapes_backslash_quote_becomes_quote() {
        // \\\" → "
        assert_eq!(process_raw_string_escapes("a\\\"b"), "a\"b");
    }

    #[test]
    fn test_process_raw_string_escapes_other_escapes_kept_as_is() {
        // \\n stays as \\n (not converted to newline)
        assert_eq!(process_raw_string_escapes("a\\nb"), "a\\nb");
        // \\t stays as \\t
        assert_eq!(process_raw_string_escapes("a\\tb"), "a\\tb");
    }

    #[test]
    fn test_process_raw_string_escapes_trailing_backslash_kept() {
        assert_eq!(process_raw_string_escapes("a\\"), "a\\");
    }

    #[test]
    fn test_process_raw_string_escapes_no_escapes_unchanged() {
        assert_eq!(process_raw_string_escapes("hello world"), "hello world");
    }

    // ── is_flattenable_operator ───────────────────────────────────────────────

    #[test]
    fn test_is_flattenable_operator_plus_and_mul() {
        assert!(is_flattenable_operator("+"));
        assert!(is_flattenable_operator("*"));
    }

    #[test]
    fn test_is_flattenable_operator_others_are_not() {
        assert!(!is_flattenable_operator("-"));
        assert!(!is_flattenable_operator("/"));
        assert!(!is_flattenable_operator("&&"));
    }

    // ── is_comparison_operator ────────────────────────────────────────────────

    #[test]
    fn test_is_comparison_operator_all_comparison_ops() {
        for op in ["<", ">", "<=", ">=", "==", "!=", "===", "!=="] {
            assert!(is_comparison_operator(op), "{op} should be comparison");
        }
    }

    #[test]
    fn test_is_comparison_operator_non_comparison() {
        assert!(!is_comparison_operator("+"));
        assert!(!is_comparison_operator("&&"));
        assert!(!is_comparison_operator("="));
    }

    // ── is_broadcast_op ──────────────────────────────────────────────────────

    #[test]
    fn test_is_broadcast_op_dot_plus() {
        assert!(is_broadcast_op(".+"));
        assert!(is_broadcast_op(".*"));
        assert!(is_broadcast_op(".=="));
    }

    #[test]
    fn test_is_broadcast_op_plain_op_is_not() {
        assert!(!is_broadcast_op("+"));
        assert!(!is_broadcast_op("."));  // single dot is not broadcast
    }

    // ── strip_broadcast_dot ───────────────────────────────────────────────────

    #[test]
    fn test_strip_broadcast_dot_removes_leading_dot() {
        assert_eq!(strip_broadcast_dot(".+"), "+");
        assert_eq!(strip_broadcast_dot(".*"), "*");
        assert_eq!(strip_broadcast_dot(".=="), "==");
    }

    #[test]
    fn test_strip_broadcast_dot_non_broadcast_unchanged() {
        assert_eq!(strip_broadcast_dot("+"), "+");
        assert_eq!(strip_broadcast_dot("=="), "==");
    }

    // ── map_binary_op ─────────────────────────────────────────────────────────

    #[test]
    fn test_map_binary_op_known_ops() {
        assert_eq!(map_binary_op("+"), Some(BinaryOp::Add));
        assert_eq!(map_binary_op("-"), Some(BinaryOp::Sub));
        assert_eq!(map_binary_op("*"), Some(BinaryOp::Mul));
        assert_eq!(map_binary_op("/"), Some(BinaryOp::Div));
        assert_eq!(map_binary_op("=="), Some(BinaryOp::Eq));
        assert_eq!(map_binary_op("!="), Some(BinaryOp::Ne));
        assert_eq!(map_binary_op("<"), Some(BinaryOp::Lt));
        assert_eq!(map_binary_op(">"), Some(BinaryOp::Gt));
        assert_eq!(map_binary_op("<="), Some(BinaryOp::Le));
        assert_eq!(map_binary_op(">="), Some(BinaryOp::Ge));
        assert_eq!(map_binary_op("&&"), Some(BinaryOp::And));
        assert_eq!(map_binary_op("||"), Some(BinaryOp::Or));
        assert_eq!(map_binary_op("==="), Some(BinaryOp::Egal));
        assert_eq!(map_binary_op("!=="), Some(BinaryOp::NotEgal));
        assert_eq!(map_binary_op("<:"), Some(BinaryOp::Subtype));
    }

    #[test]
    fn test_map_binary_op_unknown_returns_none() {
        assert_eq!(map_binary_op(".+"), None);
        assert_eq!(map_binary_op("unknown"), None);
        assert_eq!(map_binary_op(""), None);
    }

    // ── map_unary_op ──────────────────────────────────────────────────────────

    #[test]
    fn test_map_unary_op_known_ops() {
        assert_eq!(map_unary_op("-"), Some(UnaryOp::Neg));
        assert_eq!(map_unary_op("+"), Some(UnaryOp::Pos));
        assert_eq!(map_unary_op("!"), Some(UnaryOp::Not));
    }

    #[test]
    fn test_map_unary_op_unknown_returns_none() {
        assert_eq!(map_unary_op("*"), None);
        assert_eq!(map_unary_op("~"), None);
        assert_eq!(map_unary_op(""), None);
    }

    // ── map_builtin_name ──────────────────────────────────────────────────────

    #[test]
    fn test_map_builtin_name_known_builtins() {
        assert_eq!(map_builtin_name("sqrt"), Some(BuiltinOp::Sqrt));
        assert_eq!(map_builtin_name("length"), Some(BuiltinOp::Length));
        assert_eq!(map_builtin_name("push!"), Some(BuiltinOp::Push));
        assert_eq!(map_builtin_name("typeof"), Some(BuiltinOp::TypeOf));
        assert_eq!(map_builtin_name("isa"), Some(BuiltinOp::Isa));
    }

    #[test]
    fn test_map_builtin_name_unknown_returns_none() {
        assert_eq!(map_builtin_name("foo"), None);
        assert_eq!(map_builtin_name("haskey"), None);  // now Pure Julia
        assert_eq!(map_builtin_name(""), None);
    }
}
