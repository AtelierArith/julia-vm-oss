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
pub(in crate::vm) mod helpers;
mod ir_conversion;
mod parse;

use crate::builtins::BuiltinId;
use crate::rng::RngLike;
use crate::vm::value::is_native_array_value;
use crate::vm::value::StrRef;

use super::error::VmError;
use super::formatting::Resolved;
use super::stack_ops::StackOps;
use super::value::{
    array_wrapper_value_to_array_value, native_array_value_ref, ArrayValue, ExprValue, SymbolValue,
    Value,
};
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
                // Symbol(a, b, ...) - concatenate string forms of all
                // arguments and form a single Symbol. Mirrors upstream
                // Julia's `Base.Symbol(args...) = Symbol(string(args...))`
                // (Issue #4780). The 1-arg fast path stays in
                // `vm/exec/call.rs` for the common `Symbol("name")`
                // case.
                if argc == 1 {
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
                            // Single non-String/Symbol arg: stringify via
                            // the print-form helper. Issue #5038: resolve
                            // `Value::StructRef` against the struct heap
                            // first so `Symbol(::struct)` / `Symbol(::Pair)`
                            // render via their show form (e.g. "1 => 2")
                            // instead of leaking the Rust debug
                            // `StructRef(heap_idx=N)` repr into the symbol
                            // name.
                            let resolved = crate::vm::formatting::resolve_struct_refs_for_format(
                                &val,
                                &self.struct_heap,
                            );
                            let s = crate::vm::formatting::format_value_print(&Resolved::trivial(
                                &resolved,
                            ));
                            self.stack.push(Value::Symbol(SymbolValue::new(s)));
                        }
                    }
                } else {
                    // Pop argc values, format each via the print-form
                    // helper, concatenate, then form the Symbol.
                    let mut parts = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        parts.push(self.stack.pop_value()?);
                    }
                    parts.reverse();
                    let mut joined = String::new();
                    for v in &parts {
                        // Issue #5038: resolve heap-allocated StructRefs
                        // before formatting so e.g.
                        // `Symbol("a_", Pair(1, 2), "_b")` does not leak the
                        // Rust debug `StructRef(heap_idx=N)` repr.
                        let resolved = crate::vm::formatting::resolve_struct_refs_for_format(
                            v,
                            &self.struct_heap,
                        );
                        joined.push_str(&crate::vm::formatting::format_value_print(
                            &Resolved::trivial(&resolved),
                        ));
                    }
                    self.stack.push(Value::Symbol(SymbolValue::new(joined)));
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
                            // Native Array splat — routed through the
                            // file-local `native_array_value_ref` helper while
                            // the runtime migrates to Memory-first storage and
                            // Pure Julia `Array{T,N}` wrappers (Issue #3908).
                            // The `other =>` arm below preserves
                            // exhaustiveness.
                            ref arg_ref if is_native_array_value(arg_ref) => {
                                let Some(arr) = native_array_value_ref(arg_ref) else {
                                    return Err(VmError::TypeError(
                                        "splat: expected Array".to_string(),
                                    ));
                                };
                                // Convert array elements to Values
                                let borrowed = arr.borrow();
                                for i in 0..borrowed.element_count() {
                                    if let Ok(val) = borrowed.get_linear(i) {
                                        final_args.push(val);
                                    }
                                }
                            }
                            Value::Generator(generator) => {
                                let collected =
                                    self.collect_iterator(&Value::Generator(generator))?;
                                let Some(arr) = array_wrapper_value_to_array_value(
                                    &collected,
                                    &self.struct_heap,
                                )?
                                else {
                                    return Err(VmError::TypeError(format!(
                                        "Cannot splat eager generator materialized as {:?}",
                                        collected.value_type()
                                    )));
                                };
                                for i in 0..arr.element_count() {
                                    final_args.push(arr.get_linear(i)?);
                                }
                            }
                            other => {
                                if let Some(arr) =
                                    array_wrapper_value_to_array_value(&other, &self.struct_heap)?
                                {
                                    for i in 0..arr.element_count() {
                                        final_args.push(arr.get_linear(i)?);
                                    }
                                    continue;
                                }

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
                        Value::Str(s) => s.to_string(),
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
                // esc(expr) - wrap in Expr(:escape, expr), matching upstream Julia.
                // Macro expansion lowering consumes this marker to suppress module
                // hygiene for caller-scope subtrees (Issue #7631).
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "esc requires exactly 1 argument".to_string(),
                    ));
                }
                let arg = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                self.stack
                    .push(Value::Expr(ExprValue::from_head("escape", vec![arg])));
            }

            BuiltinId::Eval => {
                // eval(expr) - evaluate an Expr AST at runtime. The two-argument
                // form carries the target module: either the module value from
                // the explicit `Core.eval(m, expr)` / `eval(m, expr)` spelling,
                // or the compile-time module path the compiler attaches to a
                // bare `eval(expr)` inside a module body so bindings land in
                // that module instead of Main (Issue #11421).
                if argc != 1 && argc != 2 {
                    return Err(VmError::TypeError(
                        "eval requires 1 or 2 arguments".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                let module_name = if argc == 2 {
                    let module_val = self.stack.pop_value()?;
                    let name = match &module_val {
                        Value::Module(module) => module.name.clone(),
                        Value::Str(s) => s.to_string(),
                        other => {
                            return Err(VmError::TypeError(format!(
                                "eval target must be a Module, got {:?}",
                                other.value_type()
                            )));
                        }
                    };
                    // Main is the default global scope: keep the historical
                    // unqualified-global path for it.
                    (crate::module_names::classify_builtin_module(&name)
                        != crate::module_names::BuiltinModule::Main)
                        .then_some(name)
                } else {
                    None
                };
                let result = self.eval_module_expr_value(&val, module_name.as_deref())?;
                self.stack.push(result);
            }

            BuiltinId::ThrowMethodErrorWithArgs => {
                // Stack: [args..., message, fname]. Raise the
                // compile-time-detected dispatch miss with its typed payload
                // so a caught MethodError exposes upstream's `.f`/`.args`
                // (Issue #11374). The message is the exact compile-time text.
                if argc < 2 {
                    return Err(VmError::InternalError(
                        "_throw_method_error_with_args requires message and fname".to_string(),
                    ));
                }
                let fname = match self.stack.pop_value()? {
                    Value::Str(s) => s.to_string(),
                    other => {
                        return Err(VmError::InternalError(format!(
                            "_throw_method_error_with_args fname must be Str, got {:?}",
                            other.value_type()
                        )));
                    }
                };
                let message = match self.stack.pop_value()? {
                    Value::Str(s) => s.to_string(),
                    other => {
                        return Err(VmError::InternalError(format!(
                            "_throw_method_error_with_args message must be Str, got {:?}",
                            other.value_type()
                        )));
                    }
                };
                let mut args = Vec::with_capacity(argc - 2);
                for _ in 0..(argc - 2) {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();
                return Err(self.method_error_with_payload(message, &fname, &args));
            }

            BuiltinId::GeneratedEval => {
                if argc != 1 {
                    return Err(VmError::TypeError(
                        "_generated_eval requires exactly 1 argument".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                let result = self.eval_generated_expr_value(&val)?;
                self.stack.push(result);
            }

            BuiltinId::EvalDefinedCall => {
                // Trampoline body for a method defined by runtime `eval` of
                // a quoted function definition (Issue #8647). The current
                // frame already has its parameter values bound into slots
                // by the normal call machinery; look up this function
                // index's stored body and re-enter the tree-walking `eval`
                // interpreter over it, which resolves those parameters by
                // name. Mirrors `BuiltinId::GeneratedEval` above, minus the
                // compiled-generator step in front.
                if argc != 0 {
                    return Err(VmError::InternalError(
                        "_eval_defined_call takes no explicit arguments".to_string(),
                    ));
                }
                let func_index = self
                    .frames
                    .last()
                    .and_then(|frame| frame.func_index)
                    .ok_or_else(|| {
                        VmError::InternalError(
                            "_eval_defined_call: no active function frame".to_string(),
                        )
                    })?;
                let entry = self.eval_defined_bodies.get(&func_index).ok_or_else(|| {
                    VmError::InternalError(format!(
                        "_eval_defined_call: no eval-defined body registered for function index {}",
                        func_index
                    ))
                })?;
                let body = entry.body.clone();
                let module_name = entry.module_name.clone();
                let result = self.eval_expr_value_with_module(&body, module_name.as_deref())?;
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
                    Value::I64(n) => n,
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
                let tuple = crate::vm::value::TupleValue::new(vec![result, Value::I64(next_pos)]);
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
                                Some(expected_n) => expr.nargs() == expected_n,
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
                    Value::Str(s) => s.to_string(),
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
                    Value::Str(s) => s.to_string(),
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
                    Value::Str(s) => s.to_string(),
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
                    Value::Str(s) => s.to_string(),
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
                    Value::Str(s) => s.to_string(),
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
                    Value::Str(s) => s.to_string(),
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
                    self.any_test_failed = true; // Issue #8191: drives non-zero CLI exit.
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
                    Value::Str(s) => s.to_string(),
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
                    self.any_test_failed = true; // Issue #8191: drives non-zero CLI exit.
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

            BuiltinId::TestRecordError => {
                // _test_record_error!(msg, detail) - record an errored test outcome
                // (Issue #10093): the `@test` expression threw an exception (or
                // evaluated to a non-Boolean value), mirroring upstream
                // `Test.Error` / `do_test`'s `Threw` branch. Errored is a distinct
                // outcome from a recorded failure, but both drive a non-zero CLI
                // exit via the sticky `any_test_failed` flag.
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "_test_record_error! requires exactly 2 arguments: _test_record_error!(msg, detail)"
                            .to_string(),
                    ));
                }
                let detail = self.stack.pop_value()?;
                let msg = self.stack.pop_value()?;

                let msg_str = match msg {
                    Value::Str(s) => s.to_string(),
                    _ => format!("{:?}", msg),
                };
                let detail_str = match detail {
                    Value::Str(s) => s.to_string(),
                    _ => format!("{:?}", detail),
                };

                self.test_error_count += 1;
                self.any_test_failed = true; // Issue #8191: drives non-zero CLI exit.
                self.emit_output(&format!("  Error During Test: {}", msg_str), true);
                self.emit_output(&format!("    {}", detail_str), true);
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
                    Value::Str(s) => s.to_string(),
                    _ => format!("{:?}", name),
                };

                // Push an enclosing-counts frame and reset the counters, so a
                // nested testset tracks only its own tests (Issue #10338).
                self.testset_begin_frame(name_str.clone());
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

                // Counts of the set that just finished; the pop also folds
                // them into the enclosing testset's restored counters, so an
                // outer set's own end aggregates its nested sets
                // (Issue #10338).
                let (pass, fail, errored, broken) = self.testset_end_frame();
                let total = pass + fail + errored + broken;
                // Optional counters appear only when non-zero, in upstream
                // `TestSetException` order: passed, failed, errored, broken
                // (Issue #10093).
                let mut summary = format!("  {} passed, {} failed", pass, fail);
                if errored > 0 {
                    summary.push_str(&format!(", {} errored", errored));
                }
                if broken > 0 {
                    summary.push_str(&format!(", {} broken", broken));
                }
                summary.push_str(&format!(" ({} total)", total));
                self.emit_output(&summary, true);
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
                        Value::Str(s) => s.to_string(),
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
                    Ok(regex) => self.stack.push(Value::Regex(Box::new(regex))),
                    Err(e) => return Err(VmError::TypeError(format!("Invalid regex: {}", e))),
                }
            }

            BuiltinId::RegexMatch => {
                // match(regex, string) or match(regex, string, start) - find the
                // first match (from a 1-based byte offset for the 3-arg form),
                // returns RegexMatch or nothing (Issue #10178).
                if !(2..=3).contains(&argc) {
                    return Err(VmError::TypeError(
                        "match requires 2 or 3 arguments: match(regex, string[, start])"
                            .to_string(),
                    ));
                }
                // 3-arg form: `start` (1-based byte index) is on top of the stack.
                let start = if argc == 3 {
                    match self.stack.pop_value()? {
                        Value::I64(i) => Some(i),
                        Value::F64(n) if n.fract() == 0.0 => Some(n as i64),
                        other => {
                            return Err(VmError::TypeError(format!(
                                "Third argument to match must be an Integer, got {:?}",
                                other
                            )))
                        }
                    }
                } else {
                    None
                };
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
                let matched = match start {
                    // Julia passes `idx - 1` as the 0-based byte offset to PCRE.
                    // `idx < 1` is a BoundsError upstream; reject it rather than
                    // underflow the usize offset.
                    Some(idx) if idx < 1 => {
                        return Err(VmError::TypeError(format!(
                            "match: start index {} out of range (must be >= 1)",
                            idx
                        )))
                    }
                    // A start past `ncodeunits(s) + 1` is a "bad offset value"
                    // ErrorException upstream (PCRE.exec rejects the offset),
                    // not a silent `nothing` (Issue #10736).
                    Some(idx) if idx > string.len() as i64 + 1 => {
                        return Err(VmError::ErrorException(
                            "PCRE.exec error: bad offset value".to_string(),
                        ))
                    }
                    Some(idx) => regex.find_from(&string, (idx - 1) as usize),
                    None => regex.find(&string),
                };
                match matched {
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

            BuiltinId::EndsWithRegex => {
                // _endswith_regex(string, regex) - true iff `regex` matches ending at
                // the end of `string` (emulates PCRE ENDANCHORED). Internal helper for
                // the pure-Julia `endswith(s, ::Regex)` method (Issue #5676).
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "_endswith_regex requires 2 arguments: _endswith_regex(string, regex)"
                            .to_string(),
                    ));
                }
                let regex = match self.stack.pop_value()? {
                    Value::Regex(r) => r,
                    _ => {
                        return Err(VmError::TypeError(
                            "Second argument to endswith must be a Regex".to_string(),
                        ))
                    }
                };
                let string = match self.stack.pop_value()? {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(
                            "First argument to endswith must be a String".to_string(),
                        ))
                    }
                };
                self.stack.push(Value::Bool(regex.ends_with_match(&string)));
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
                // Upstream treats a plain String replacement as LITERAL text:
                // only a SubstitutionString (s"..."), which routes through
                // `_replace_general` / `ExpandSubstitution`, expands capture
                // references. fancy-regex's replacement syntax treats `$` as a
                // capture reference, so escape it (`$` -> `$$`) before handing
                // the literal replacement to the engine (Issue #10721).
                let replacement = replacement.replace('$', "$$");
                let result = if count == 0 {
                    regex.replace_all(&string, &replacement)
                } else if count == 1 {
                    regex.replace(&string, &replacement)
                } else {
                    regex.replacen(&string, count as usize, &replacement)
                };
                self.stack.push(Value::str_new(result));
            }

            BuiltinId::ExpandSubstitution => {
                // Expand a SubstitutionString replacement, resolving
                // \N / \g<name> / \0 capture references and C-escapes
                // (Issue #10174). Mirrors upstream Base._replace(io,
                // ::SubstitutionString, str, r, re) in julia/base/regex.jl.
                // Two shapes:
                //   _expand_substitution(subst, regex_match, regex)  — Regex pattern
                //   _expand_substitution(subst, matched_str, nothing) — non-Regex pattern
                // (the non-Regex form only allows \0 / \g<0> for the whole match).
                if argc != 3 {
                    return Err(VmError::TypeError(
                        "_expand_substitution requires 3 arguments: _expand_substitution(subst, match, regex_or_nothing)"
                            .to_string(),
                    ));
                }
                let third = self.stack.pop_value()?;
                let second = self.stack.pop_value()?;
                let subst = match self.stack.pop_value()? {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(
                            "First argument to _expand_substitution must be a String".to_string(),
                        ))
                    }
                };
                let expanded = match (second, third) {
                    (Value::RegexMatch(m), Value::Regex(regex)) => {
                        crate::vm::value::expand_substitution(&subst, &m, &regex)
                    }
                    (Value::Str(matched), Value::Nothing) => {
                        crate::vm::value::expand_substitution_plain(&subst, &matched)
                    }
                    _ => {
                        return Err(VmError::TypeError(
                            "_expand_substitution: second/third args must be (RegexMatch, Regex) or (String, Nothing)"
                                .to_string(),
                        ))
                    }
                }
                .map_err(VmError::ErrorException)?;
                self.stack.push(Value::str_new(expanded));
            }

            BuiltinId::RegexMatchFrom => {
                // _regex_match_from(regex, string, byteindex) - the
                // findnext(re, str, i) primitive used by the multi-pattern
                // `replace` scan (Issue #10175). Returns the first match at or
                // after the 1-based byte index, or `nothing`.
                if argc != 3 {
                    return Err(VmError::TypeError(
                        "_regex_match_from requires 3 arguments: _regex_match_from(regex, string, byteindex)"
                            .to_string(),
                    ));
                }
                let start = match self.stack.pop_value()? {
                    Value::I64(n) => n,
                    _ => {
                        return Err(VmError::TypeError(
                            "Third argument to _regex_match_from must be an Int64".to_string(),
                        ))
                    }
                };
                let string = match self.stack.pop_value()? {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(
                            "Second argument to _regex_match_from must be a String".to_string(),
                        ))
                    }
                };
                let regex = match self.stack.pop_value()? {
                    Value::Regex(r) => r,
                    _ => {
                        return Err(VmError::TypeError(
                            "First argument to _regex_match_from must be a Regex".to_string(),
                        ))
                    }
                };
                // 1-based Julia byte index → 0-based search start; clamp a
                // non-positive index to 0 (the canonical `find_from` returns
                // None once `pos` runs past the end of the string).
                let pos = usize::try_from(start - 1).unwrap_or(0);
                match regex.find_from(&string, pos) {
                    Some(m) => self.stack.push(Value::RegexMatch(Box::new(m))),
                    None => self.stack.push(Value::Nothing),
                }
            }

            BuiltinId::RegexSplit => {
                // _regex_split(string, regex, limit, keepempty) - split string by
                // regex delimiter (Issue #10176). Args pushed in that order, so
                // pop in reverse.
                if argc != 4 {
                    return Err(VmError::TypeError(
                        "_regex_split requires 4 arguments: _regex_split(string, regex, limit, keepempty)"
                            .to_string(),
                    ));
                }
                let keepempty = match self.stack.pop_value()? {
                    Value::Bool(b) => b,
                    Value::I64(n) => n != 0,
                    _ => {
                        return Err(VmError::TypeError(
                            "keepempty argument to split must be a Bool".to_string(),
                        ))
                    }
                };
                let limit = match self.stack.pop_value()? {
                    Value::I64(n) => n,
                    _ => {
                        return Err(VmError::TypeError(
                            "limit argument to split must be an Integer".to_string(),
                        ))
                    }
                };
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
                let parts: Vec<StrRef> = regex
                    .split_with(&string, limit, keepempty)
                    .into_iter()
                    .map(StrRef::from)
                    .collect();
                let len = parts.len();
                let arr = ArrayValue::memory_first_from_strings(parts, vec![len]);
                self.push_array_value_as_wrapper(arr)?;
            }

            BuiltinId::RegexEachmatch => {
                // eachmatch(regex, string; overlap=false) - return all matches as Vector.
                // The optional `overlap::Bool` keyword (Issue #10199) is threaded
                // as a third stack value (top of stack).
                if argc != 2 && argc != 3 {
                    return Err(VmError::TypeError(
                        "eachmatch requires 2 arguments: eachmatch(regex, string)".to_string(),
                    ));
                }
                let overlap = if argc == 3 {
                    match self.stack.pop_value()? {
                        Value::Bool(b) => b,
                        _ => {
                            return Err(VmError::TypeError(
                                "eachmatch overlap keyword must be a Bool".to_string(),
                            ))
                        }
                    }
                } else {
                    false
                };
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
                let found = if overlap {
                    regex.find_all_overlapping(&string)
                } else {
                    regex.find_all(&string)
                };
                let matches: Vec<Value> = found
                    .into_iter()
                    .map(|m| Value::RegexMatch(Box::new(m)))
                    .collect();
                let arr = ArrayValue::any_vector(matches);
                self.push_array_value_as_wrapper(arr)?;
            }

            BuiltinId::RegexFindnext => {
                // _regex_findnext(regex, string, i) - first match at or after the
                // 1-based byte index `i`, returned as a RegexMatch (or Nothing).
                // Backs the pure-Julia findnext(::Regex, s, i) / findfirst(::Regex, s)
                // methods (Issue #10177), mirroring upstream `_findnext_re`'s
                // PCRE.exec(re, str, idx-1) positional search against the full string.
                if argc != 3 {
                    return Err(VmError::TypeError(
                        "_regex_findnext requires 3 arguments: _regex_findnext(regex, string, i)"
                            .to_string(),
                    ));
                }
                let i = match self.stack.pop_value()? {
                    Value::I64(n) => n,
                    _ => {
                        return Err(VmError::TypeError(
                            "Third argument to _regex_findnext must be an Int64".to_string(),
                        ))
                    }
                };
                let string = match self.stack.pop_value()? {
                    Value::Str(s) => s,
                    _ => {
                        return Err(VmError::TypeError(
                            "Second argument to _regex_findnext must be a String".to_string(),
                        ))
                    }
                };
                let regex = match self.stack.pop_value()? {
                    Value::Regex(r) => r,
                    _ => {
                        return Err(VmError::TypeError(
                            "First argument to _regex_findnext must be a Regex".to_string(),
                        ))
                    }
                };
                // 1-based Julia byte index → 0-based byte position. `i < 1` is a
                // degenerate index (the pure-Julia wrapper bounds-checks the upper
                // end); saturate to avoid an unsigned underflow panic — a huge
                // `pos` simply yields no match (`captures_from_pos` returns None).
                let pos = if i >= 1 { (i - 1) as usize } else { usize::MAX };
                match regex.find_from(&string, pos) {
                    Some(m) => self.stack.push(Value::RegexMatch(Box::new(m))),
                    None => self.stack.push(Value::Nothing),
                }
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
