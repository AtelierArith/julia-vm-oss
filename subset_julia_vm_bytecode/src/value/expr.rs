//! `Expr` AST value type for metaprogramming.
//!
//! Split out of `container.rs` by value kind (Issue #6835).

use super::array_value::{native_array_ref_value, new_array_ref, ArrayRef};
use super::macro_::SymbolValue;
use super::ArrayValue;
use super::Value;

/// Julia Expr - an AST node for metaprogramming
///
/// In Julia: `Expr(:call, :+, 1, 2)` represents `1 + 2`
///
/// Structure:
/// - `head`: Symbol indicating the node type (:call, :block, :if, etc.)
/// - `args`: Vector of child nodes (Expr, Symbol, literals)
#[derive(Debug, Clone)]
pub struct ExprValue {
    /// The expression head (e.g., :call, :block, :if, :quote)
    pub head: SymbolValue,
    /// Child arguments (can be Expr, Symbol, or literal values).
    ///
    /// Upstream Julia defines `Expr` as a mutable object with
    /// `args::Array{Any,1}` (`julia/base/boot.jl`, `julia/src/builtins.c`).
    /// Keep the same reference semantics: `ex.args` returns this shared array,
    /// so mutating it updates the owning Expr.
    pub args: ArrayRef,
}

impl ExprValue {
    pub fn new(head: SymbolValue, args: Vec<Value>) -> Self {
        Self {
            head,
            args: new_array_ref(ArrayValue::any_vector(args)),
        }
    }

    /// Create an Expr from a head string and args
    pub fn from_head(head: impl AsRef<str>, args: Vec<Value>) -> Self {
        Self {
            head: SymbolValue::new(head),
            args: new_array_ref(ArrayValue::any_vector(args)),
        }
    }

    /// Check if this expression has the given head
    pub fn is_head(&self, head: &str) -> bool {
        self.head.as_str() == head
    }

    /// Get the head as a Symbol value
    pub fn get_head(&self) -> Value {
        Value::Symbol(self.head.clone())
    }

    /// Get args as an array value
    pub fn get_args(&self) -> Value {
        native_array_ref_value(self.args.clone())
    }

    /// Snapshot args into a Vec for read-only consumers.
    pub fn args_snapshot(&self) -> Vec<Value> {
        self.args
            .borrow()
            .to_logical_value_vec()
            .unwrap_or_default()
    }

    /// Get argument at 1-based index (Julia convention)
    pub fn get_arg(&self, index: usize) -> Option<Value> {
        if index >= 1 && index <= self.nargs() {
            self.args.borrow().get_linear(index - 1).ok()
        } else {
            None
        }
    }

    /// Get the number of arguments
    pub fn nargs(&self) -> usize {
        self.args.borrow().element_count()
    }
}

impl std::fmt::Display for ExprValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Expr(:{}", self.head.as_str())?;
        for arg in self.args_snapshot() {
            write!(f, ", ")?;
            match &arg {
                Value::Symbol(s) => write!(f, ":{}", s.as_str())?,
                Value::I8(n) => write!(f, "{}", n)?,
                Value::I16(n) => write!(f, "{}", n)?,
                Value::I32(n) => write!(f, "{}", n)?,
                Value::I64(n) => write!(f, "{}", n)?,
                // Issue #4755: previously the catch-all `_ => write!("{:?}")`
                // arm leaked Rust's `Debug` repr (e.g. `I128(...)`) into
                // Expr display, surfaced by PR #4754 (Issue #4753) which
                // promotes overflowing integer literals to `Value::I128`.
                Value::I128(n) => write!(f, "{}", n)?,
                Value::U8(n) => write!(f, "{}", n)?,
                Value::U16(n) => write!(f, "{}", n)?,
                Value::U32(n) => write!(f, "{}", n)?,
                Value::U64(n) => write!(f, "{}", n)?,
                Value::U128(n) => write!(f, "{}", n)?,
                Value::Bool(b) => write!(f, "{}", b)?,
                Value::F32(n) => write!(f, "{}", n)?,
                Value::F64(n) => write!(f, "{}", n)?,
                Value::Str(s) => write!(f, "\"{}\"", s)?,
                Value::Char(c) => write!(f, "'{}'", c)?,
                Value::Nothing => write!(f, "nothing")?,
                Value::Missing => write!(f, "missing")?,
                Value::Expr(e) => write!(f, "{}", e)?,
                _ => write!(f, "{:?}", arg)?,
            }
        }
        write!(f, ")")
    }
}
