//! Macro system builtin functions for the VM.
//!
//! Metaprogramming operations: Symbol, Expr, gensym, QuoteNode, esc, eval.
//!
//! # Module Organization
//!
//! - `helpers.rs`: Helper functions for Meta.isidentifier / Meta.isoperator
//! - `eval.rs`: Expression evaluation (eval() builtin)
//! - `parse.rs`: String parsing (Meta.parse, include_string)
//! - `ir_conversion.rs`: IR conversion (Meta.lower, source-string round-tripping)

// SAFETY: i64→u64 cast for splat_mask is a reinterpretation of a bitmask value;
// i64→usize casts are for string/regex positions known to be non-negative from caller.
#![allow(clippy::cast_sign_loss)]

mod eval;
mod helpers;
mod ir_conversion;
mod parse;

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::value::{ExprValue, SymbolValue, Value};
use super::Vm;

use helpers::{
    is_binary_operator, is_operator, is_postfix_operator, is_unary_operator, is_valid_identifier,
};

impl<R: RngLike> Vm<R> {
    /// Execute macro system builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a macro builtin.
    pub(super) fn execute_builtin_macro(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::SymbolNew => {
                // Symbol("name") - create a Symbol from string
                let val = self.stack.pop_value()?;
                match val {
                    Value::Str(s) => {
                        self.stack.push(Value::Symbol(SymbolValue::new(s)));
                    }
                    Value::Symbol(s) => {
                        // Symbol(sym) returns the symbol unchanged
                        self.stack.push(Value::Symbol(s));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "Symbol: expected String or Symbol, got {:?}",
                            val.value_type()
                        )));
                    }
                }
            }

            BuiltinId::ExprNew => {
                // Expr(head, args...) - create an Expr AST node
                // First arg is head (Symbol), rest are args
                if argc < 1 {
                    return Err(VmError::TypeError(
                        "Expr requires at least 1 argument (head)".to_string(),
                    ));
                }

                // Pop all args in reverse order
                let mut args = Vec::with_capacity(argc - 1);
                for _ in 0..(argc - 1) {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse(); // Restore correct order

                // Pop the head
                let head_val = self.stack.pop_value()?;
                let head = match head_val {
                    Value::Symbol(s) => s,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "Expr: head must be a Symbol, got {:?}",
                            head_val.value_type()
                        )));
                    }
                };

                self.stack.push(Value::Expr(ExprValue::new(head, args)));
            }

            BuiltinId::ExprNewWithSplat => {
                // Expr(head, args...) with splat expansion at runtime
                // Stack: [head, arg0, arg1, ..., argN, splat_mask]
                // argc includes the splat_mask, so actual args = argc - 1
                if argc < 2 {
                    return Err(VmError::TypeError(
                        "ExprNewWithSplat requires at least head and splat_mask".to_string(),
                    ));
                }

                // Pop splat_mask (last argument)
                let splat_mask = match self.stack.pop_value()? {
                    Value::I64(v) => v as u64,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "ExprNewWithSplat: splat_mask must be I64, got {:?}",
                            other.value_type()
                        )));
                    }
                };

                // Pop the remaining args (argc - 2 since we exclude head and splat_mask)
                let arg_count = argc - 2;
                let mut raw_args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    raw_args.push(self.stack.pop_value()?);
                }
                raw_args.reverse(); // Restore correct order

                // Pop the head
                let head_val = self.stack.pop_value()?;
                let head = match head_val {
                    Value::Symbol(s) => s,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "Expr: head must be a Symbol, got {:?}",
                            head_val.value_type()
                        )));
                    }
                };

                // Expand args according to splat_mask
                let mut final_args = Vec::new();
                for (i, arg) in raw_args.into_iter().enumerate() {
                    // Note: bit (i+1) corresponds to args[i] because bit 0 is for head
                    let is_splat = (splat_mask & (1u64 << (i + 1))) != 0;
                    if is_splat {
                        // Expand tuple or array
                        match arg {
                            Value::Tuple(tuple) => {
                                // Clone elements from the tuple
                                final_args.extend(tuple.elements.iter().cloned());
                            }
                            Value::Array(arr) => {
                                // Convert array elements to Values
                                let borrowed = arr.borrow();
                                for i in 0..borrowed.len() {
                                    if let Some(val) = borrowed.data.get_value(i) {
                                        final_args.push(val);
                                    }
                                }
                            }
                            other => {
                                // If not iterable, error
                                return Err(VmError::TypeError(format!(
                                    "Cannot splat value of type {:?}",
                                    other.value_type()
                                )));
                            }
                        }
                    } else {
                        final_args.push(arg);
                    }
                }

                self.stack
                    .push(Value::Expr(ExprValue::new(head, final_args)));
            }

            BuiltinId::Gensym => {
                // gensym() or gensym("base") or gensym(:base) - generate unique symbol for hygiene
                let sym_name = if argc == 0 {
                    // Generate default name: ##123
                    let counter = self.gensym_counter;
                    self.gensym_counter += 1;
                    format!("##{}", counter)
                } else {
                    // gensym("base") or gensym(:base) generates ##base#123
                    let arg = self.stack.pop().ok_or_else(|| {
                        VmError::TypeError("gensym: missing argument".to_string())
                    })?;
                    let base = match arg {
                        Value::Str(s) => s,
                        Value::Symbol(s) => s.as_str().to_string(),
                        _ => {
                            return Err(VmError::TypeError(
                                "gensym: expected String or Symbol".to_string(),
                            ))
                        }
                    };
                    let counter = self.gensym_counter;
                    self.gensym_counter += 1;
                    format!("##{}#{}", base, counter)
                };
                self.stack.push(Value::Symbol(SymbolValue::new(sym_name)));
            }

            BuiltinId::QuoteNodeNew => {
                // QuoteNode(value) - wrap value in QuoteNode
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "QuoteNode requires exactly 1 argument".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                self.stack.push(Value::QuoteNode(Box::new(val)));
            }

            BuiltinId::LineNumberNodeNew => {
                // LineNumberNode(line) or LineNumberNode(line, file)
                use crate::vm::LineNumberNodeValue;

                match argc {
                    1 => {
                        // LineNumberNode(line) - file is None
                        let line_val = self.stack.pop_value()?;
                        let line = match line_val {
                            Value::I64(n) => n,
                            Value::F64(n) if n.fract() == 0.0 => n as i64,
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "LineNumberNode line must be an integer, got {:?}",
                                    line_val
                                )));
                            }
                        };
                        self.stack.push(Value::LineNumberNode(LineNumberNodeValue {
                            line,
                            file: None,
                        }));
                    }
                    2 => {
                        // LineNumberNode(line, file) - args are [line, file] on stack (file is on top)
                        let file_val = self.stack.pop_value()?;
                        let line_val = self.stack.pop_value()?;
                        let line = match line_val {
                            Value::I64(n) => n,
                            Value::F64(n) if n.fract() == 0.0 => n as i64,
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "LineNumberNode line must be an integer, got {:?}",
                                    line_val
                                )));
                            }
                        };
                        let file = match file_val {
                            Value::Symbol(s) => Some(s.as_str().to_string()),
                            Value::Nothing => None,
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "LineNumberNode file must be a Symbol or nothing, got {:?}",
                                    file_val
                                )));
                            }
                        };
                        self.stack
                            .push(Value::LineNumberNode(LineNumberNodeValue { line, file }));
                    }
                    _ => {
                        return Err(VmError::TypeError(
                            "LineNumberNode requires 1 or 2 arguments".to_string(),
                        ));
                    }
                }
            }

            BuiltinId::GlobalRefNew => {
                // GlobalRef(mod, name) - create a global reference
                use crate::vm::GlobalRefValue;

                if argc != 2 {
                    return Err(VmError::TypeError(
                        "GlobalRef requires exactly 2 arguments: GlobalRef(mod, name)".to_string(),
                    ));
                }
                // Args are [mod, name] on stack (name is on top)
                let name_val = self.stack.pop_value()?;
                let mod_val = self.stack.pop_value()?;

                // Extract module name
                let module = match mod_val {
                    Value::Module(m) => m.name.clone(),
                    Value::Symbol(s) => s.as_str().to_string(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "GlobalRef mod must be a Module or Symbol, got {:?}",
                            mod_val
                        )));
                    }
                };

                // Extract symbol name
                let name = match name_val {
                    Value::Symbol(s) => s,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "GlobalRef name must be a Symbol, got {:?}",
                            name_val
                        )));
                    }
                };

                self.stack
                    .push(Value::GlobalRef(GlobalRefValue::new(module, name)));
            }

            BuiltinId::Esc => {
                // esc(expr) - escape expression for macro hygiene
                // Hygiene is handled during lowering/quote processing.
                // At runtime, esc returns its argument unchanged.
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "esc requires exactly 1 argument".to_string(),
                    ));
                }
                // Argument is already on the stack; keep it as the return value.
            }

            BuiltinId::Eval => {
                // eval(expr) - evaluate an Expr AST at runtime
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "eval requires exactly 1 argument".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                let result = self.eval_expr_value(&val)?;
                self.stack.push(result);
            }

            BuiltinId::MacroExpand | BuiltinId::MacroExpandBang => {
                // macroexpand(m, x) and macroexpand!(m, x) - return expanded form of macro call
                // In SubsetJuliaVM, macro expansion happens at compile time during lowering.
                // At runtime, we receive expressions that have already been expanded.
                // For a quoted macro call like :(@time 1+1), we return the expression as-is
                // since runtime expansion is not supported (requires access to macro definitions).
                // The module parameter is accepted for API compatibility but ignored.
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "macroexpand requires exactly 2 arguments: macroexpand(m, x)".to_string(),
                    ));
                }
                // Pop the expression (second argument)
                let expr = self.stack.pop_value()?;
                // Pop the module (first argument, ignored)
                let _module = self.stack.pop_value()?;
                // Return the expression unchanged
                // Note: In full Julia, this would expand macros in the expression.
                // SubsetJuliaVM performs macro expansion at compile time, so runtime
                // expressions are already expanded or represent unevaluated macro calls.
                self.stack.push(expr);
            }

            BuiltinId::IncludeString => {
                // include_string(m, code) or include_string(m, code, filename)
                // Parse and evaluate all expressions in the code string.
                // Returns the value of the last expression.
                if !(2..=3).contains(&argc) {
                    return Err(VmError::TypeError(
                        "include_string requires 2 or 3 arguments: include_string(m, code) or include_string(m, code, filename)".to_string(),
                    ));
                }
                // Pop arguments in reverse order
                let _filename = if argc == 3 {
                    self.stack.pop_str()?
                } else {
                    "string".to_string()
                };
                let code = self.stack.pop_str()?;
                let _module = self.stack.pop_value()?; // Module ignored in SubsetJuliaVM

                // Parse and evaluate all expressions in the code
                let result = self.include_string_impl(&code)?;
                self.stack.push(result);
            }

            BuiltinId::EvalFile => {
                // evalfile(path) or evalfile(path, args)
                // Read file and evaluate all expressions.
                if !(1..=2).contains(&argc) {
                    return Err(VmError::TypeError(
                        "evalfile requires 1 or 2 arguments: evalfile(path) or evalfile(path, args)"
                            .to_string(),
                    ));
                }
                // Pop arguments in reverse order
                let _args = if argc == 2 {
                    self.stack.pop_value()? // args ignored in SubsetJuliaVM
                } else {
                    Value::Nothing
                };
                let path = self.stack.pop_str()?;

                // Read file contents
                let code = std::fs::read_to_string(&path).map_err(|e| {
                    VmError::ErrorException(format!("evalfile: cannot read file '{}': {}", path, e))
                })?;

                // Parse and evaluate all expressions
                let result = self.include_string_impl(&code)?;
                self.stack.push(result);
            }

            BuiltinId::MetaParse => {
                // _meta_parse(str) - parse string to Expr (for Meta.parse)
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "_meta_parse requires exactly 1 argument".to_string(),
                    ));
                }
                let str_val = self.stack.pop_str()?;
                let result = self.parse_string_to_value(&str_val)?;
                self.stack.push(result);
            }

            BuiltinId::MetaParseAt => {
                // _meta_parse_at(str, pos) - parse at position, return (expr, next_pos)
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "_meta_parse_at requires exactly 2 arguments".to_string(),
                    ));
                }
                let pos = match self.stack.pop_value()? {
                    Value::I64(n) => n as usize,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_meta_parse_at: position must be Int, got {:?}",
                            other.value_type()
                        )));
                    }
                };
                let str_val = self.stack.pop_str()?;
                let (result, next_pos) = self.parse_string_at_to_value(&str_val, pos)?;
                // Return tuple (expr, next_pos)
                let tuple =
                    crate::vm::value::TupleValue::new(vec![result, Value::I64(next_pos as i64)]);
                self.stack.push(Value::Tuple(tuple));
            }

            BuiltinId::MetaIsExpr => {
                // Meta.isexpr(ex, head) or Meta.isexpr(ex, head, n)
                // Returns true if ex is an Expr with the given head (and optionally length n)
                if !(2..=3).contains(&argc) {
                    return Err(VmError::TypeError(
                        "Meta.isexpr requires 2 or 3 arguments".to_string(),
                    ));
                }

                // Pop in reverse order: n (optional), head, ex
                let n = if argc == 3 {
                    match self.stack.pop_value()? {
                        Value::I64(v) => Some(v as usize),
                        other => {
                            return Err(VmError::TypeError(format!(
                                "Meta.isexpr: third argument must be Int, got {:?}",
                                other.value_type()
                            )));
                        }
                    }
                } else {
                    None
                };

                let head = self.stack.pop_value()?;
                let ex = self.stack.pop_value()?;

                // Check if ex is an Expr with matching head
                let result = match &ex {
                    Value::Expr(expr) => {
                        let head_matches = match &head {
                            Value::Symbol(s) => expr.head.as_str() == s.as_str(),
                            _ => false,
                        };
                        if head_matches {
                            // If n is specified, also check args length
                            match n {
                                Some(expected_n) => expr.args.len() == expected_n,
                                None => true,
                            }
                        } else {
                            false
                        }
                    }
                    _ => false,
                };

                self.stack.push(Value::Bool(result));
            }

            BuiltinId::MetaQuot => {
                // Meta.quot(ex) - wrap expression in :quote Expr
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "Meta.quot requires exactly 1 argument".to_string(),
                    ));
                }
                let ex = self.stack.pop_value()?;
                let quoted = ExprValue::from_head("quote", vec![ex]);
                self.stack.push(Value::Expr(quoted));
            }

            BuiltinId::MetaIsIdentifier => {
                // Meta.isidentifier(s) - check if string/symbol is a valid identifier
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "Meta.isidentifier requires exactly 1 argument".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                let s = match val {
                    Value::Symbol(sym) => sym.as_str().to_string(),
                    Value::Str(s) => s,
                    _ => {
                        // Non-string/symbol returns false
                        self.stack.push(Value::Bool(false));
                        return Ok(Some(()));
                    }
                };
                let result = is_valid_identifier(&s);
                self.stack.push(Value::Bool(result));
            }

            BuiltinId::MetaIsOperator => {
                // Meta.isoperator(s) - check if symbol/string can be used as an operator
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "Meta.isoperator requires exactly 1 argument".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                let s = match val {
                    Value::Symbol(sym) => sym.as_str().to_string(),
                    Value::Str(s) => s,
                    _ => {
                        self.stack.push(Value::Bool(false));
                        return Ok(Some(()));
                    }
                };
                let result = is_operator(&s);
                self.stack.push(Value::Bool(result));
            }

            BuiltinId::MetaIsUnaryOperator => {
                // Meta.isunaryoperator(s) - check if can be used as unary operator
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "Meta.isunaryoperator requires exactly 1 argument".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                let s = match val {
                    Value::Symbol(sym) => sym.as_str().to_string(),
                    Value::Str(s) => s,
                    _ => {
                        self.stack.push(Value::Bool(false));
                        return Ok(Some(()));
                    }
                };
                let result = is_unary_operator(&s);
                self.stack.push(Value::Bool(result));
            }

            BuiltinId::MetaIsBinaryOperator => {
                // Meta.isbinaryoperator(s) - check if can be used as binary operator
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "Meta.isbinaryoperator requires exactly 1 argument".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                let s = match val {
                    Value::Symbol(sym) => sym.as_str().to_string(),
                    Value::Str(s) => s,
                    _ => {
                        self.stack.push(Value::Bool(false));
                        return Ok(Some(()));
                    }
                };
                let result = is_binary_operator(&s);
                self.stack.push(Value::Bool(result));
            }

            BuiltinId::MetaIsPostfixOperator => {
                // Meta.ispostfixoperator(s) - check if can be used as postfix operator
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "Meta.ispostfixoperator requires exactly 1 argument".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                let s = match val {
                    Value::Symbol(sym) => sym.as_str().to_string(),
                    Value::Str(s) => s,
                    _ => {
                        self.stack.push(Value::Bool(false));
                        return Ok(Some(()));
                    }
                };
                let result = is_postfix_operator(&s);
                self.stack.push(Value::Bool(result));
            }

            BuiltinId::MetaLower => {
                // _meta_lower(expr) - lower expression to Core IR
                // Takes an Expr/Symbol/literal and returns the lowered representation
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "_meta_lower requires exactly 1 argument".to_string(),
                    ));
                }
                let expr_val = self.stack.pop_value()?;
                let result = self.lower_value_to_ir(&expr_val)?;
                self.stack.push(result);
            }

            // Test operations (for Pure Julia @test/@testset/@test_throws macros)
            BuiltinId::TestRecord => {
                // _test_record!(passed, msg) - record test result
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "_test_record! requires exactly 2 arguments: _test_record!(passed, msg)"
                            .to_string(),
                    ));
                }
                let msg = self.stack.pop_value()?;
                let passed = self.stack.pop_value()?;

                let msg_str = match msg {
                    Value::Str(s) => s,
                    _ => format!("{:?}", msg),
                };
                let passed_bool = match passed {
                    Value::Bool(b) => b,
                    _ => {
                        return Err(VmError::TypeError(
                            "First argument to _test_record! must be a Bool".to_string(),
                        ))
                    }
                };

                if passed_bool {
                    self.test_pass_count += 1;
                    self.emit_output(&format!("  Test Passed: {}", msg_str), true);
                } else {
                    self.test_fail_count += 1;
                    self.emit_output(&format!("  Test Failed: {}", msg_str), true);
                }
                self.stack.push(Value::Nothing);
            }

            BuiltinId::TestRecordBroken => {
                // _test_record_broken!(passed, msg) - record broken test result
                // If passed=true, this is an error (test unexpectedly passed - no longer broken!)
                // If passed=false, this is expected (test is broken as expected)
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "_test_record_broken! requires exactly 2 arguments: _test_record_broken!(passed, msg)"
                            .to_string(),
                    ));
                }
                let msg = self.stack.pop_value()?;
                let passed = self.stack.pop_value()?;

                let msg_str = match msg {
                    Value::Str(s) => s,
                    _ => format!("{:?}", msg),
                };
                let passed_bool = match passed {
                    Value::Bool(b) => b,
                    _ => {
                        return Err(VmError::TypeError(
                            "First argument to _test_record_broken! must be a Bool".to_string(),
                        ))
                    }
                };

                if passed_bool {
                    // Test unexpectedly passed - this is an error!
                    self.test_fail_count += 1;
                    self.emit_output(
                        &format!("  Test Error (unexpectedly passed): {}", msg_str),
                        true,
                    );
                } else {
                    // Test failed as expected - this is a broken test
                    self.test_broken_count += 1;
                    self.emit_output(&format!("  Test Broken: {}", msg_str), true);
                }
                self.stack.push(Value::Nothing);
            }

            BuiltinId::TestSetBegin => {
                // _testset_begin!(name) - begin test set
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "_testset_begin! requires exactly 1 argument: _testset_begin!(name)"
                            .to_string(),
                    ));
                }
                let name = self.stack.pop_value()?;
                let name_str = match name {
                    Value::Str(s) => s,
                    _ => format!("{:?}", name),
                };

                self.current_testset = Some(name_str.clone());
                self.test_pass_count = 0;
                self.test_fail_count = 0;
                self.test_broken_count = 0;
                self.emit_output(&format!("Test Set: {}", name_str), true);
                self.stack.push(Value::Nothing);
            }

            BuiltinId::TestSetEnd => {
                // _testset_end!() - end test set and print summary
                if argc != 0 {
                    return Err(VmError::TypeError(
                        "_testset_end! takes no arguments".to_string(),
                    ));
                }

                let total = self.test_pass_count + self.test_fail_count + self.test_broken_count;
                if self.test_broken_count > 0 {
                    self.emit_output(
                        &format!(
                            "  {} passed, {} failed, {} broken ({} total)",
                            self.test_pass_count,
                            self.test_fail_count,
                            self.test_broken_count,
                            total
                        ),
                        true,
                    );
                } else {
                    self.emit_output(
                        &format!(
                            "  {} passed, {} failed ({} total)",
                            self.test_pass_count, self.test_fail_count, total
                        ),
                        true,
                    );
                }
                self.current_testset = None;
                self.stack.push(Value::Nothing);
            }

            // Regex operations
            BuiltinId::RegexNew => {
                // Regex(pattern) or Regex(pattern, flags) - create regex
                use crate::vm::value::RegexValue;
                if !(1..=2).contains(&argc) {
                    return Err(VmError::TypeError(
                        "Regex requires 1 or 2 arguments: Regex(pattern) or Regex(pattern, flags)"
                            .to_string(),
                    ));
                }
                let flags = if argc == 2 {
                    match self.stack.pop_value()? {
                        Value::Str(s) => s,
                        _ => {
                            return Err(VmError::TypeError(
                                "Regex flags must be a String".to_string(),
                            ))
                        }
                    }
                } else {
                    String::new()
                };
                let pattern = match self.stack.pop_value()? {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(
                            "Regex pattern must be a String".to_string(),
                        ))
                    }
                };
                match RegexValue::new(&pattern, &flags) {
                    Ok(regex) => self.stack.push(Value::Regex(regex)),
                    Err(e) => return Err(VmError::TypeError(format!("Invalid regex: {}", e))),
                }
            }

            BuiltinId::RegexMatch => {
                // match(regex, string) - find first match, returns RegexMatch or nothing
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "match requires 2 arguments: match(regex, string)".to_string(),
                    ));
                }
                let string = match self.stack.pop_value()? {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(
                            "Second argument to match must be a String".to_string(),
                        ))
                    }
                };
                let regex = match self.stack.pop_value()? {
                    Value::Regex(r) => r,
                    _ => {
                        return Err(VmError::TypeError(
                            "First argument to match must be a Regex".to_string(),
                        ))
                    }
                };
                match regex.find(&string) {
                    Some(m) => self.stack.push(Value::RegexMatch(Box::new(m))),
                    None => self.stack.push(Value::Nothing),
                }
            }

            BuiltinId::RegexOccursin => {
                // occursin(regex, string) - check if regex matches anywhere in string
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "occursin requires 2 arguments: occursin(regex, string)".to_string(),
                    ));
                }
                let string = match self.stack.pop_value()? {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(
                            "Second argument to occursin must be a String".to_string(),
                        ))
                    }
                };
                let regex = match self.stack.pop_value()? {
                    Value::Regex(r) => r,
                    _ => {
                        return Err(VmError::TypeError(
                            "First argument to occursin must be a Regex".to_string(),
                        ))
                    }
                };
                self.stack.push(Value::Bool(regex.is_match(&string)));
            }

            BuiltinId::RegexReplace => {
                // _regex_replace(string, regex, replacement, count)
                // count=0 means replace all, count=N means replace at most N
                if argc != 4 {
                    return Err(VmError::TypeError(
                        "_regex_replace requires 4 arguments: _regex_replace(string, regex, replacement, count)"
                            .to_string(),
                    ));
                }
                let count = match self.stack.pop_value()? {
                    Value::I64(n) => n,
                    _ => {
                        return Err(VmError::TypeError(
                            "Fourth argument to _regex_replace must be an Int64".to_string(),
                        ))
                    }
                };
                let replacement = match self.stack.pop_value()? {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(
                            "Third argument to _regex_replace must be a String".to_string(),
                        ))
                    }
                };
                let regex = match self.stack.pop_value()? {
                    Value::Regex(r) => r,
                    _ => {
                        return Err(VmError::TypeError(
                            "Second argument to _regex_replace must be a Regex".to_string(),
                        ))
                    }
                };
                let string = match self.stack.pop_value()? {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(
                            "First argument to _regex_replace must be a String".to_string(),
                        ))
                    }
                };
                let result = if count == 0 {
                    regex.replace_all(&string, &replacement)
                } else if count == 1 {
                    regex.replace(&string, &replacement)
                } else {
                    regex.replacen(&string, count as usize, &replacement)
                };
                self.stack.push(Value::Str(result));
            }

            BuiltinId::RegexSplit => {
                // split(string, regex) - split string by regex
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "split requires 2 arguments: split(string, regex)".to_string(),
                    ));
                }
                let regex = match self.stack.pop_value()? {
                    Value::Regex(r) => r,
                    _ => {
                        return Err(VmError::TypeError(
                            "Second argument to split must be a Regex".to_string(),
                        ))
                    }
                };
                let string = match self.stack.pop_value()? {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(
                            "First argument to split must be a String".to_string(),
                        ))
                    }
                };
                let parts: Vec<Value> = regex
                    .split(&string)
                    .into_iter()
                    .map(|s| Value::Str(s.to_string()))
                    .collect();
                use crate::vm::value::{ArrayData, ArrayValue};
                let len = parts.len();
                let arr = ArrayValue::new(
                    ArrayData::String(
                        parts
                            .into_iter()
                            .filter_map(|v| if let Value::Str(s) = v { Some(s) } else { None })
                            .collect(),
                    ),
                    vec![len],
                );
                self.stack
                    .push(Value::Array(crate::vm::value::new_array_ref(arr)));
            }

            BuiltinId::RegexEachmatch => {
                // eachmatch(regex, string) - return all matches as Vector
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "eachmatch requires 2 arguments: eachmatch(regex, string)".to_string(),
                    ));
                }
                let string = match self.stack.pop_value()? {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(
                            "Second argument to eachmatch must be a String".to_string(),
                        ))
                    }
                };
                let regex = match self.stack.pop_value()? {
                    Value::Regex(r) => r,
                    _ => {
                        return Err(VmError::TypeError(
                            "First argument to eachmatch must be a Regex".to_string(),
                        ))
                    }
                };
                let matches: Vec<Value> = regex
                    .find_all(&string)
                    .into_iter()
                    .map(|m| Value::RegexMatch(Box::new(m)))
                    .collect();
                use crate::vm::value::{ArrayData, ArrayValue};
                let len = matches.len();
                let arr = ArrayValue::new(ArrayData::Any(matches), vec![len]);
                self.stack
                    .push(Value::Array(crate::vm::value::new_array_ref(arr)));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
