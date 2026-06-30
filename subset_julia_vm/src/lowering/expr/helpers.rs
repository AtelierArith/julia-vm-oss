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
///
/// The subtype operators `<:` and `>:` chain the same way (Issue #5492):
/// `A <: B <: C` lowers to `(A <: B) && (B <: C)`, matching upstream Julia's
/// `expand-compare-chain` in `julia/src/julia-syntax.scm`, which treats every
/// comparison-precedence operator (including `<:`/`>:`) uniformly.
pub(crate) fn is_comparison_operator(op: &str) -> bool {
    matches!(
        op,
        "<" | ">" | "<=" | ">=" | "==" | "!=" | "===" | "!==" | "<:" | ">:"
    )
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
    let fn_ref = Expr::FunctionRef {
        name: fn_name.to_string(),
        span,
    };
    make_broadcasted_call_with_callee(fn_ref, args, span)
}

pub(crate) fn make_broadcasted_call_with_callee(callee: Expr, args: Vec<Expr>, span: Span) -> Expr {
    // Strip materialize wrappers from args that are broadcast results (fusion)
    let fused_args: Vec<Expr> = args.into_iter().map(strip_materialize).collect();

    // Build: Broadcasted(fn_ref, (arg1, arg2, ...))
    let args_tuple = Expr::TupleLiteral {
        elements: fused_args,
        span,
    };
    let broadcasted_call = Expr::Call {
        function: "Broadcasted".to_string(),
        args: vec![callee, args_tuple],
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
        "<=" | "≤" => Some(BinaryOp::Le),
        ">=" | "≥" => Some(BinaryOp::Ge),
        "==" => Some(BinaryOp::Eq),
        "!=" | "≠" => Some(BinaryOp::Ne),
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
        // Note: sqrt is now routed through method dispatch first (Issue #3737).
        // BuiltinOp::Sqrt is still reachable via `base_function_to_builtin_op`
        // as a fallback for real numeric types when dispatch finds no match.
        // Note: ifelse is now Pure Julia (base/essentials.jl, Issue #3733). Lowering
        // to BuiltinOp::IfElse short-circuits the non-selected arm, which violates
        // Julia's strict-evaluation semantics for ifelse(condition, x, y).
        // Array builtins
        // zeros/ones are Pure Julia allocation dispatch (Issue #4036).
        // reshape is dispatch-first with a retained VM fallback (Issue #4276).
        // Note: trues, falses, fill are now Pure Julia (base/array.jl)
        // Note: length, size are now routed through method dispatch first (Issue #3736).
        // Pure Julia methods exist in base/range.jl (LinRange/StepRangeLen/OneTo/LogRange),
        // base/subarray.jl (SubArray, MatrixView), base/dict.jl, base/set.jl,
        // base/iterators.jl (Enumerate/Zip/Take/Drop/CartesianIndices/...),
        // base/broadcast.jl (Broadcasted), base/generator.jl, base/channels.jl, etc.
        // BuiltinOp::Length / BuiltinOp::Size remain reachable via
        // `base_function_to_builtin_op("length"|"size")` as a fallback for native
        // VM containers (Array, Tuple, String, Dict, Set, Range, Generator) when
        // method dispatch finds no matching Pure Julia method.
        // Note: push!, pop! removed from lowering shortcut (Issue #3739) — public
        // mutating collection calls now go through method dispatch so Pure Julia
        // methods on `Set` (base/set.jl) and `Dict{K,V}` (base/dict.jl) win over
        // the Rust builtin. Array calls still reach `BuiltinOp::Push` / `Pop` via
        // `base_function_to_builtin_op` after dispatch fails (call.rs fallback).
        // Note: zero is now routed through method dispatch first (Issue #3737).
        // BuiltinOp::Zero is still reachable via `base_function_to_builtin_op`
        // as a fallback for primitive numeric types when dispatch finds no
        // matching Pure Julia method.
        // Note: complex, real, imag, conj, abs, abs2, transpose are Pure Julia
        // RNG constructors
        "StableRNG" => BuiltinOp::StableRNG,
        "Xoshiro" => BuiltinOp::XoshiroRNG,
        "MersenneTwister" => BuiltinOp::MersenneTwisterRNG,
        // Normal distribution
        "randn" => BuiltinOp::Randn,
        // Tuple operations
        // Note: first/last are now Pure Julia (Issue #3734) - they dispatch through
        // the method table to the implementations in base/tuple.jl, base/range.jl,
        // base/strings/basic.jl, base/iterators.jl, and any user-defined struct methods.
        // Dict operations
        // Note: haskey, get, merge, keys, values, pairs are now Pure Julia (Issue #2572, #2573, #2669)
        // Note: delete! removed from lowering shortcut (Issue #3739) — Set and
        // Dict{K,V} have Pure Julia `delete!` methods. The Rust HashMap-backed
        // `Value::Dict` still falls back to `BuiltinOp::DictDelete` via
        // `base_function_to_builtin_op` when method dispatch finds no match.
        // Broadcasting control
        "Ref" => BuiltinOp::Ref,
        // Type operations
        "typeof" => BuiltinOp::TypeOf,
        "isa" => BuiltinOp::Isa,
        // Iterator Protocol
        // Note: iterate, collect are now routed through method dispatch first
        // (Issue #3735). Pure Julia methods exist in base/iterators.jl,
        // base/range.jl, base/generator.jl, base/dict.jl, base/channels.jl,
        // base/subarray.jl. BuiltinOp::Iterate / BuiltinOp::Collect remain
        // reachable via base_function_to_builtin_op as the fallback for
        // primitive containers (Array, Tuple, String, Range) when method
        // dispatch finds no matching Pure Julia method.
        // Macro hygiene
        // Note: gensym is now Pure Julia (meta.jl) - Issue #294
        "esc" => BuiltinOp::Esc,
        // Metaprogramming
        "eval" => BuiltinOp::Eval,
        "macroexpand" => BuiltinOp::MacroExpand,
        "macroexpand!" => BuiltinOp::MacroExpandBang,
        // Note: include_string and evalfile are now dispatched to Pure Julia (Issue #3738)
        // The Pure Julia implementations in base/meta.jl compose Meta.parse, eval, and read.
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
    fn test_is_comparison_operator_subtype_ops_chain() {
        // `<:`/`>:` are comparison-precedence operators that chain like the
        // scalar comparisons (Issue #5492): `A <: B <: C` => `(A <: B) && (B <: C)`.
        assert!(is_comparison_operator("<:"));
        assert!(is_comparison_operator(">:"));
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
        assert!(!is_broadcast_op(".")); // single dot is not broadcast
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
        assert_eq!(map_binary_op("≠"), Some(BinaryOp::Ne));
        assert_eq!(map_binary_op("<"), Some(BinaryOp::Lt));
        assert_eq!(map_binary_op(">"), Some(BinaryOp::Gt));
        assert_eq!(map_binary_op("<="), Some(BinaryOp::Le));
        assert_eq!(map_binary_op("≤"), Some(BinaryOp::Le));
        assert_eq!(map_binary_op(">="), Some(BinaryOp::Ge));
        assert_eq!(map_binary_op("≥"), Some(BinaryOp::Ge));
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
        assert_eq!(map_builtin_name("typeof"), Some(BuiltinOp::TypeOf));
        assert_eq!(map_builtin_name("isa"), Some(BuiltinOp::Isa));
    }

    #[test]
    fn test_map_builtin_name_unknown_returns_none() {
        assert_eq!(map_builtin_name("foo"), None);
        assert_eq!(map_builtin_name("haskey"), None); // now Pure Julia
                                                      // sqrt and zero now dispatch through method tables first (Issue #3737)
                                                      // and only reach BuiltinOp::Sqrt / BuiltinOp::Zero via the
                                                      // `base_function_to_builtin_op` fallback path.
        assert_eq!(map_builtin_name("sqrt"), None);
        assert_eq!(map_builtin_name("zero"), None);
        // zeros/ones now dispatch through method tables first (Issue #4036).
        assert_eq!(map_builtin_name("zeros"), None);
        assert_eq!(map_builtin_name("ones"), None);
        // length/size now dispatch through method tables first (Issue #3736)
        // and only reach BuiltinOp::Length / BuiltinOp::Size via the
        // `base_function_to_builtin_op` fallback path.
        assert_eq!(map_builtin_name("length"), None);
        assert_eq!(map_builtin_name("size"), None);
        // iterate/collect now dispatch through method tables first (Issue #3735)
        // and only reach BuiltinOp::Iterate / BuiltinOp::Collect via the
        // `base_function_to_builtin_op` fallback path.
        assert_eq!(map_builtin_name("iterate"), None);
        assert_eq!(map_builtin_name("collect"), None);
        // push!, pop!, delete! removed from lowering shortcut (Issue #3739).
        // Reachable via `base_function_to_builtin_op` fallback when method
        // dispatch cannot find a Pure Julia method.
        assert_eq!(map_builtin_name("push!"), None);
        assert_eq!(map_builtin_name("pop!"), None);
        assert_eq!(map_builtin_name("delete!"), None);
        assert_eq!(map_builtin_name(""), None);
    }
}
