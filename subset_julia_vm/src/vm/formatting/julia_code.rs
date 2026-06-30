//! `value_to_julia_code` / `expr_to_julia_string` — Julia source stringification.
//!
//! Split out of `formatting.rs` by category (Issue #6835).

use super::super::value::{ExprValue, Value};
use super::*;
use crate::expr_heads::ExprHead;
use crate::vm::builtins_macro::helpers::is_valid_identifier;
use crate::vm::value::is_native_array_value;

/// Operator precedence table (higher = binds tighter)
/// Based on Julia's operator precedence
#[inline]
pub(super) fn operator_precedence(op: &str) -> i32 {
    match op {
        // Assignment (lowest)
        "=" | "+=" | "-=" | "*=" | "/=" | "\\=" | "^=" | "&=" | "|=" | "÷=" | "%=" => 1,
        // Pair
        "=>" => 2,
        // Ternary
        "?" => 3,
        // Or
        "||" => 4,
        // And
        "&&" => 5,
        // Comparison
        "<" | ">" | "<=" | ">=" | "==" | "!=" | "===" | "!==" | "<:" | ">:" | "≤" | "≥" | "≠"
        | "≡" | "≢" => 6,
        // Range
        ":" => 7,
        // Plus
        "+" | "-" | "|" | "⊻" => 11,
        // Times
        "*" | "/" | "÷" | "%" | "&" | "\\" => 12,
        // Rational
        "//" => 13,
        // Power
        "^" => 14,
        // Type declaration
        "::" => 15,
        // Dot
        "." => 17,
        // Not an operator
        _ => 0,
    }
}

/// Check if operator is a unary operator
#[inline]
pub(super) fn is_unary_op(op: &str) -> bool {
    matches!(op, "+" | "-" | "!" | "~" | "¬" | "√" | "∛" | "∜")
}

/// Format a bare `Symbol` name for Julia-source output.
///
/// Issue #7676: when a symbol's name is not a valid Julia identifier and is not
/// an operator, upstream Julia prints it as a non-standard identifier
/// `var"name"` (see `Base.show_sym` / `Meta.is_valid_identifier` in
/// `julia/base/show.jl` and `julia/base/meta.jl`). This is what lets a quoted
/// `var"@q"` round-trip back to `var"@q"` instead of the bare `@q` (which is not
/// re-parseable). Operators (`+`, `::`, …) are left bare to preserve the
/// formatter's existing operator handling.
pub(super) fn format_symbol_name(name: &str) -> String {
    use crate::vm::builtins_macro::helpers::is_valid_identifier;
    if is_valid_identifier(name)
        || operator_precedence(name) > 0
        || is_unary_op(name)
        || name.starts_with('\'')
    {
        return name.to_string();
    }
    // Mirror Julia's `escape_raw_string`: only `\` and `"` are escaped inside a
    // `var"..."` literal.
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    format!("var\"{}\"", escaped)
}

/// Convert a Value to Julia code format string (used recursively in expressions)
pub(crate) fn value_to_julia_code(val: &Value) -> String {
    match val {
        Value::Symbol(s) => format_symbol_name(s.as_str()),
        Value::Expr(e) => expr_to_julia_string(e),
        Value::I64(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I8(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U8(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        Value::F64(n) => {
            if n.fract() == 0.0 && n.is_finite() {
                format!("{:.1}", n)
            } else {
                n.to_string()
            }
        }
        Value::F32(n) => n.to_string(),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::Str(s) => format!("\"{}\"", s),
        Value::Char(c) => format!("'{}'", c),
        Value::Nothing => "nothing".to_string(),
        Value::Missing => "missing".to_string(),
        Value::QuoteNode(inner) => match inner.as_ref() {
            Value::Symbol(sym) => quotenode_symbol_to_julia_code(sym.as_str()),
            other => format!("$(QuoteNode({}))", value_to_julia_code(other)),
        },
        Value::LineNumberNode(ln) => ln.to_string(),
        Value::GlobalRef(gr) => gr.to_string(),
        Value::Tuple(t) => {
            let parts: Vec<String> = t.elements.iter().map(value_to_julia_code).collect();
            if parts.len() == 1 {
                format!("({},)", parts[0])
            } else {
                format!("({})", parts.join(", "))
            }
        }
        // Issue #4722: Core.SimpleVector source form is `Core.svec(...)`.
        Value::SimpleVector(sv) => {
            let parts: Vec<String> = sv.elements.iter().map(value_to_julia_code).collect();
            format!("Core.svec({})", parts.join(", "))
        }
        // Route the legacy native array carrier through
        // `native_array_value_ref` so the unwrap stays centralized while #3908
        // retires the native container.
        _ if is_native_array_value(val) => {
            let Some(arr) = native_array_value_ref(val) else {
                return format_value(val);
            };
            format_array_value_with(arr, |_idx, v| value_to_julia_code(v))
        }
        Value::Memory(mem) => {
            let mem = mem.borrow();
            let values: Vec<Value> = (0..mem.len())
                .map(|idx| mem.data.get_value(idx).unwrap_or(Value::Nothing))
                .collect();
            let parts: Vec<String> = values.iter().take(100).map(value_to_julia_code).collect();
            if values.len() > 100 {
                format!("[{}, ...]", parts.join(", "))
            } else {
                format!("[{}]", parts.join(", "))
            }
        }
        // Fall back to format_value for other types
        _ => format_value(val),
    }
}

fn quotenode_symbol_to_julia_code(name: &str) -> String {
    if is_valid_identifier(name) {
        format!(":{}", name)
    } else {
        format!("Symbol(\"{}\")", escape_var_string_symbol(name))
    }
}

fn escape_var_string_symbol(name: &str) -> String {
    name.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Convert an Expr to Julia code format string
pub(crate) fn expr_to_julia_string(expr: &ExprValue) -> String {
    let head_name = expr.head.as_str();
    let args = expr.args_snapshot();

    match ExprHead::from_name(head_name) {
        Some(ExprHead::Call) => format_call(&args),
        Some(ExprHead::Tuple) => format_tuple(&args),
        Some(ExprHead::Vect) => format_vect(&args),
        Some(ExprHead::Ref) => format_ref(&args),
        Some(ExprHead::Dot) => format_dot(&args),
        Some(ExprHead::Block) => format_block(&args),
        Some(ExprHead::Quote) => format_quote(&args),
        Some(ExprHead::Comparison) => format_comparison(&args),
        Some(ExprHead::AndAnd) => format_short_circuit(ExprHead::AndAnd.as_str(), &args),
        Some(ExprHead::OrOr) => format_short_circuit(ExprHead::OrOr.as_str(), &args),
        Some(ExprHead::If) => format_if(&args),
        Some(ExprHead::Assign) => format_assignment(&args),
        Some(ExprHead::Kw) => format_kw(&args),
        Some(ExprHead::Parameters) => format_parameters(&args),
        Some(ExprHead::Curly) => format_curly(&args),
        Some(ExprHead::String) => format_string_interpolation(&args),
        Some(ExprHead::MacroCall) => format_macrocall(&args),
        // Fallback: show as Expr(...) for unsupported heads
        _ => {
            let args_str: Vec<String> = args.iter().map(value_to_julia_code).collect();
            if args_str.is_empty() {
                format!("Expr(:{})", head_name)
            } else {
                format!("Expr(:{}, {})", head_name, args_str.join(", "))
            }
        }
    }
}

/// Format a :call expression
fn format_call(args: &[Value]) -> String {
    if args.is_empty() {
        return "()".to_string();
    }

    // First argument is the function/operator
    let func = &args[0];
    let func_name = match func {
        Value::Symbol(s) => s.as_str(),
        // For non-symbol callables, use function call syntax
        _ => {
            let func_str = value_to_julia_code(func);
            let func_args = &args[1..];
            let args_str: Vec<String> = func_args.iter().map(value_to_julia_code).collect();
            return format!("({})({})", func_str, args_str.join(", "));
        }
    };

    let prec = operator_precedence(func_name);
    let func_args = &args[1..];

    // Binary operator with 2 arguments
    if prec > 0 && func_args.len() == 2 {
        let left = value_to_julia_code(&func_args[0]);
        let right = value_to_julia_code(&func_args[1]);
        format!("{} {} {}", left, func_name, right)
    }
    // Unary operator with 1 argument
    else if is_unary_op(func_name) && func_args.len() == 1 {
        let operand = value_to_julia_code(&func_args[0]);
        // Check if operand needs parentheses (if it's a complex expression)
        if matches!(&func_args[0], Value::Expr(_)) {
            format!("{}({})", func_name, operand)
        } else {
            format!("{}{}", func_name, operand)
        }
    }
    // N-ary operators like + and * with more than 2 arguments
    else if (func_name == "+" || func_name == "*") && func_args.len() > 2 {
        let parts: Vec<String> = func_args.iter().map(value_to_julia_code).collect();
        parts.join(&format!(" {} ", func_name))
    }
    // Range operator :
    else if func_name == ":" && (func_args.len() == 2 || func_args.len() == 3) {
        let parts: Vec<String> = func_args.iter().map(value_to_julia_code).collect();
        parts.join(":")
    }
    // Regular function call
    else {
        let args_str: Vec<String> = func_args.iter().map(value_to_julia_code).collect();
        format!("{}({})", func_name, args_str.join(", "))
    }
}

/// Format a :tuple expression
fn format_tuple(args: &[Value]) -> String {
    let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
    if parts.len() == 1 {
        format!("({},)", parts[0])
    } else {
        format!("({})", parts.join(", "))
    }
}

/// Format a :vect expression (array literal)
fn format_vect(args: &[Value]) -> String {
    let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
    format!("[{}]", parts.join(", "))
}

/// Format a :ref expression (indexing)
fn format_ref(args: &[Value]) -> String {
    if args.is_empty() {
        return "[]".to_string();
    }
    let array = value_to_julia_code(&args[0]);
    let indices: Vec<String> = args[1..].iter().map(value_to_julia_code).collect();
    format!("{}[{}]", array, indices.join(", "))
}

/// Format a :. (dot) expression (field access or broadcasting)
fn format_dot(args: &[Value]) -> String {
    if args.len() >= 2 {
        let obj = value_to_julia_code(&args[0]);
        // Second arg could be QuoteNode or Symbol for field access
        let field = match &args[1] {
            Value::QuoteNode(inner) => value_to_julia_code(inner),
            Value::Symbol(s) => s.as_str().to_string(),
            other => value_to_julia_code(other),
        };
        format!("{}.{}", obj, field)
    } else if args.len() == 1 {
        value_to_julia_code(&args[0])
    } else {
        ".".to_string()
    }
}

/// Format a :block expression
fn format_block(args: &[Value]) -> String {
    // Filter out LineNumberNode
    let stmts: Vec<String> = args
        .iter()
        .filter(|a| !matches!(a, Value::LineNumberNode(_)))
        .map(value_to_julia_code)
        .collect();

    if stmts.is_empty() {
        "begin\nend".to_string()
    } else if stmts.len() == 1 {
        stmts[0].clone()
    } else {
        format!("begin\n    {}\nend", stmts.join("\n    "))
    }
}

/// Format a :quote expression
fn format_quote(args: &[Value]) -> String {
    if args.len() == 1 {
        let inner = value_to_julia_code(&args[0]);
        format!(":({})", inner)
    } else {
        let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
        format!("quote {} end", parts.join("; "))
    }
}

/// Format a :comparison expression
fn format_comparison(args: &[Value]) -> String {
    // comparison has format: [left, op, right, op2, right2, ...]
    let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
    parts.join(" ")
}

/// Format && or || expression
fn format_short_circuit(op: &str, args: &[Value]) -> String {
    if args.len() == 2 {
        let left = value_to_julia_code(&args[0]);
        let right = value_to_julia_code(&args[1]);
        format!("{} {} {}", left, op, right)
    } else {
        let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
        parts.join(&format!(" {} ", op))
    }
}

/// Format an :if expression
fn format_if(args: &[Value]) -> String {
    if args.len() >= 2 {
        let cond = value_to_julia_code(&args[0]);
        let then_branch = value_to_julia_code(&args[1]);
        if args.len() >= 3 {
            let else_branch = value_to_julia_code(&args[2]);
            format!(
                "if {}\n    {}\nelse\n    {}\nend",
                cond, then_branch, else_branch
            )
        } else {
            format!("if {}\n    {}\nend", cond, then_branch)
        }
    } else {
        "if ... end".to_string()
    }
}

/// Format an := (assignment) expression
fn format_assignment(args: &[Value]) -> String {
    if args.len() == 2 {
        let lhs = value_to_julia_code(&args[0]);
        let rhs = value_to_julia_code(&args[1]);
        format!("{} = {}", lhs, rhs)
    } else {
        "= ...".to_string()
    }
}

/// Format a :kw expression (keyword argument)
fn format_kw(args: &[Value]) -> String {
    if args.len() == 2 {
        let name = value_to_julia_code(&args[0]);
        let value = value_to_julia_code(&args[1]);
        format!("{} = {}", name, value)
    } else {
        "kw(...)".to_string()
    }
}

/// Format a :parameters expression (keyword arguments after semicolon)
fn format_parameters(args: &[Value]) -> String {
    let parts: Vec<String> = args.iter().map(value_to_julia_code).collect();
    format!("; {}", parts.join(", "))
}

/// Format a :curly expression (type parameters)
fn format_curly(args: &[Value]) -> String {
    if args.is_empty() {
        return "{}".to_string();
    }
    let base = value_to_julia_code(&args[0]);
    let params: Vec<String> = args[1..].iter().map(value_to_julia_code).collect();
    format!("{}{{{}}}", base, params.join(", "))
}

/// Format a :string expression (string interpolation)
fn format_string_interpolation(args: &[Value]) -> String {
    let mut result = String::new();
    result.push('"');
    for arg in args {
        match arg {
            Value::Str(s) => result.push_str(s),
            _ => {
                result.push_str("$(");
                result.push_str(&value_to_julia_code(arg));
                result.push(')');
            }
        }
    }
    result.push('"');
    result
}

/// Format a :macrocall expression
fn format_macrocall(args: &[Value]) -> String {
    if args.is_empty() {
        return "@...".to_string();
    }
    let macro_name = match &args[0] {
        Value::Symbol(s) => s.as_str().to_string(),
        other => value_to_julia_code(other),
    };
    // Skip LineNumberNode if present (usually args[1])
    let macro_args: Vec<String> = args[1..]
        .iter()
        .filter(|a| !matches!(a, Value::LineNumberNode(_)))
        .map(value_to_julia_code)
        .collect();
    if macro_args.is_empty() {
        macro_name
    } else {
        format!("{} {}", macro_name, macro_args.join(" "))
    }
}
