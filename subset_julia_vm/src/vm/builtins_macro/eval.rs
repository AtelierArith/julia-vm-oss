//! Expression evaluation for the macro system.
//!
//! Handles eval() builtin: evaluates Expr AST nodes at runtime.

// SAFETY: i64→u32 cast in integer exponentiation is guarded by `if *exp >= 0` check.
#![allow(clippy::cast_sign_loss)]

use std::collections::HashMap;

use crate::{expr_heads::ExprHead, rng::RngLike, types::JuliaType};

use super::super::error::VmError;
use super::super::frame::Frame;
use super::super::hof_exec::state::RuntimeCallableResult;
use super::super::type_utils::type_objects_equal;
use super::super::types::FunctionInfo;
use super::super::util::bind_value_to_slot;
use super::super::value::{ArrayValue, ExprValue, FunctionValue, TupleValue, Value};
use super::super::Vm;

/// Pre-dispatch snapshot of the VM execution-state depths, captured before an
/// `eval`-driven call so it can be unwound if the call raises (Issue #7687).
///
/// An error escaping an eval dispatch surfaces as a Rust `Err` rather than being
/// unwound by a bytecode exception handler, so the failing callee leaves its
/// frame(s), operand-stack residue, return addresses, and any installed
/// try-handlers behind. These depths let the dispatch restore the pre-call
/// state so an eval caller (e.g. an `eval`-driven `try`/`catch`/`finally`)
/// resumes in its original frame.
struct EvalDispatchUnwind {
    frame_depth: usize,
    stack_depth: usize,
    return_ip_depth: usize,
    handler_depth: usize,
}

impl<R: RngLike> Vm<R> {
    pub(crate) fn remember_current_generated_expr_cache_key(
        &mut self,
        func: &FunctionInfo,
        func_index: usize,
        args: &[Value],
        eval_frame: Option<Frame>,
    ) {
        if !func.is_generated {
            return;
        }
        let depth = self.frames.len().saturating_sub(1);
        let key = eval_frame.as_ref().map_or_else(
            || self.positional_generated_expr_cache_key(func_index, args),
            |key_frame| self.runtime_generated_expr_cache_key(func_index, func, args, key_frame),
        );
        self.generated_expr_pending_keys.insert(depth, key);
        if let Some(frame) = eval_frame {
            self.generated_expr_pending_eval_frames.insert(depth, frame);
        }
    }

    pub(crate) fn eval_generated_expr_value(&mut self, expr: &Value) -> Result<Value, VmError> {
        let staged_value = Self::generated_eval_payload(expr);
        let depth = self.frames.len().saturating_sub(1);
        if let Some(key) = self.generated_expr_pending_keys.get(&depth).cloned() {
            self.generated_expr_cache.insert(key, staged_value.clone());
            if let Some(eval_frame) = self.generated_expr_pending_eval_frames.get(&depth).cloned() {
                self.try_push_temporary_call_frame(eval_frame)?;
                let result = self.eval_expr_value(&staged_value);
                self.pop_call_frame();
                return result;
            }
            return self.eval_expr_value(&staged_value);
        }

        let cache_args = {
            let Some(frame) = self.frames.last() else {
                return self.eval_expr_value(expr);
            };
            let Some(func_index) = frame.func_index else {
                return self.eval_expr_value(expr);
            };
            let Some(func) = self.functions.get(func_index) else {
                return self.eval_expr_value(expr);
            };
            if !func.is_generated {
                return self.eval_expr_value(expr);
            }
            let Some(key) = Self::generated_body_expr_cache_key_from_frame(func_index, func, frame)
            else {
                return self.eval_expr_value(expr);
            };
            key
        };
        self.generated_expr_cache
            .insert(cache_args, staged_value.clone());
        self.eval_expr_value(&staged_value)
    }

    pub(crate) fn try_eval_cached_generated_expr(
        &mut self,
        func_index: usize,
        func: &FunctionInfo,
        args: &[Value],
        frame: &Frame,
    ) -> Result<Option<Value>, VmError> {
        if !func.is_generated {
            return Ok(None);
        }
        let key = self.runtime_generated_expr_cache_key(func_index, func, args, frame);
        let Some(expr) = self.generated_expr_cache.get(&key).cloned() else {
            return Ok(None);
        };

        self.try_push_temporary_call_frame(frame.clone())?;
        let result = self.eval_expr_value(&expr);
        self.pop_call_frame();
        result.map(Some)
    }

    pub(crate) fn bind_generated_body_arg_types(
        &mut self,
        func: &FunctionInfo,
        args: &[Value],
        frame: &mut Frame,
    ) {
        if !func.is_generated {
            return;
        }

        self.bind_type_params(func, args, frame);

        let kwarg_values: Vec<(usize, Value)> = func
            .kwparams
            .iter()
            .filter_map(|kwparam| {
                let value = frame.locals_slots.get(kwparam.slot)?.as_ref()?.clone();
                Some((kwparam.slot, value))
            })
            .collect();

        if let Some(vararg_idx) = func.vararg_param_index {
            for idx in 0..vararg_idx {
                let Some(arg) = args.get(idx) else {
                    continue;
                };
                let Some(slot) = func.param_slots.get(idx) else {
                    continue;
                };
                let ty = Value::DataType(Box::new(self.dispatch_julia_type_for_value(arg)));
                bind_value_to_slot(frame, *slot, ty, &mut self.struct_heap);
            }

            let vararg_types = args[vararg_idx..]
                .iter()
                .map(|arg| Value::DataType(Box::new(self.dispatch_julia_type_for_value(arg))))
                .collect();
            let vararg_tuple = Value::Tuple(TupleValue {
                elements: vararg_types,
            });
            if let Some(slot) = func.param_slots.get(vararg_idx) {
                bind_value_to_slot(frame, *slot, vararg_tuple, &mut self.struct_heap);
            }
            self.bind_generated_body_kwarg_types(frame, &kwarg_values);
            return;
        }

        for (idx, slot) in func.param_slots.iter().enumerate() {
            let Some(arg) = args.get(idx) else {
                continue;
            };
            let ty = Value::DataType(Box::new(self.dispatch_julia_type_for_value(arg)));
            bind_value_to_slot(frame, *slot, ty, &mut self.struct_heap);
        }
        self.bind_generated_body_kwarg_types(frame, &kwarg_values);
    }

    fn bind_generated_body_kwarg_types(&mut self, frame: &mut Frame, kwargs: &[(usize, Value)]) {
        for (slot, value) in kwargs {
            let ty = Value::DataType(Box::new(self.dispatch_julia_type_for_value(value)));
            bind_value_to_slot(frame, *slot, ty, &mut self.struct_heap);
        }
    }

    fn runtime_generated_expr_cache_key(
        &self,
        func_index: usize,
        func: &FunctionInfo,
        args: &[Value],
        frame: &Frame,
    ) -> (usize, Vec<String>) {
        let mut arg_types: Vec<String> = args
            .iter()
            .map(|arg| self.dispatch_julia_type_for_value(arg).name().to_string())
            .collect();
        for kwparam in &func.kwparams {
            if let Some(Some(value)) = frame.locals_slots.get(kwparam.slot) {
                arg_types.push(format!(
                    "kw:{}={}",
                    kwparam.name,
                    self.dispatch_julia_type_for_value(value).name()
                ));
            }
        }
        (func_index, arg_types)
    }

    fn positional_generated_expr_cache_key(
        &self,
        func_index: usize,
        args: &[Value],
    ) -> (usize, Vec<String>) {
        let arg_types = args
            .iter()
            .map(|arg| self.dispatch_julia_type_for_value(arg).name().to_string())
            .collect();
        (func_index, arg_types)
    }

    fn generated_body_expr_cache_key_from_frame(
        func_index: usize,
        func: &FunctionInfo,
        frame: &Frame,
    ) -> Option<(usize, Vec<String>)> {
        let mut arg_types = Vec::new();
        for (idx, slot) in func.param_slots.iter().enumerate() {
            let value = frame.locals_slots.get(*slot)?.as_ref()?;
            if Some(idx) == func.vararg_param_index {
                let Value::Tuple(tuple) = value else {
                    return None;
                };
                arg_types.extend(
                    tuple
                        .elements
                        .iter()
                        .filter_map(Self::generated_body_type_name),
                );
            } else {
                arg_types.push(Self::generated_body_type_name(value)?);
            }
        }
        for kwparam in &func.kwparams {
            let value = frame.locals_slots.get(kwparam.slot)?.as_ref()?;
            arg_types.push(format!(
                "kw:{}={}",
                kwparam.name,
                Self::generated_body_type_name(value)?
            ));
        }
        Some((func_index, arg_types))
    }

    fn generated_body_type_name(value: &Value) -> Option<String> {
        match value {
            Value::DataType(julia_type) => Some(julia_type.name().to_string()),
            Value::Tuple(tuple) => Some(format!(
                "Tuple{{{}}}",
                tuple
                    .elements
                    .iter()
                    .filter_map(Self::generated_body_type_name)
                    .collect::<Vec<_>>()
                    .join(",")
            )),
            _ => None,
        }
    }

    fn generated_eval_payload(value: &Value) -> Value {
        match value {
            Value::QuoteNode(inner) => (**inner).clone(),
            _ => value.clone(),
        }
    }

    /// Evaluate an Expr value at runtime (for eval() builtin)
    pub(super) fn eval_expr_value(&mut self, val: &Value) -> Result<Value, VmError> {
        self.eval_expr_value_with_module(val, None)
    }

    fn eval_expr_value_with_module(
        &mut self,
        val: &Value,
        module_name: Option<&str>,
    ) -> Result<Value, VmError> {
        match val {
            // Literals evaluate to themselves
            Value::I64(n) => Ok(Value::I64(*n)),
            Value::I32(n) => Ok(Value::I32(*n)),
            Value::I16(n) => Ok(Value::I16(*n)),
            Value::I8(n) => Ok(Value::I8(*n)),
            Value::I128(n) => Ok(Value::I128(*n)),
            Value::U64(n) => Ok(Value::U64(*n)),
            Value::U32(n) => Ok(Value::U32(*n)),
            Value::U16(n) => Ok(Value::U16(*n)),
            Value::U8(n) => Ok(Value::U8(*n)),
            Value::U128(n) => Ok(Value::U128(*n)),
            Value::F64(n) => Ok(Value::F64(*n)),
            Value::F32(n) => Ok(Value::F32(*n)),
            Value::Bool(b) => Ok(Value::Bool(*b)),
            Value::Str(s) => Ok(Value::Str(s.clone())),
            Value::Char(c) => Ok(Value::Char(*c)),
            Value::Nothing => Ok(Value::Nothing),

            // QuoteNode: unwrap and return the inner value
            Value::QuoteNode(inner) => Ok((**inner).clone()),

            // Symbol: look up variable value if it exists, otherwise return as-is
            Value::Symbol(s) => {
                // The lone colon `:` is a global binding to `Colon()` in Base, so
                // `eval(Symbol(":"))` is `Colon()` upstream. A colon index inside a
                // quoted `:ref` (`:(a[:, j])`) round-trips through `Symbol(":")`
                // (Issue #7312); without this it stays a Symbol and `getindex`
                // throws a MethodError. Resolve it to `Value::SliceAll` (= `Colon()`)
                // unless a local of that name shadows it.
                if s.as_str() == ":" && self.get_variable_value(":").is_none() {
                    return Ok(Value::SliceAll);
                }
                if let Some(module_name) = module_name {
                    let qualified = format!("{}.{}", module_name, s.as_str());
                    if let Some(val) = self.get_variable_value(&qualified) {
                        return Ok(val);
                    }
                }
                // Try to resolve the symbol to a variable value
                if let Some(val) = self.get_variable_value(s.as_str()) {
                    Ok(val)
                } else {
                    // Return as-is if not a known variable (might be a function name)
                    Ok(Value::Symbol(s.clone()))
                }
            }

            // Expr: evaluate based on head
            Value::Expr(expr) => self.eval_expr_ast_with_module(expr, module_name),

            // Fallback: non-Expr/Symbol values returned as-is in eval
            other => Ok(other.clone()),
        }
    }

    fn eval_expr_ast_with_module(
        &mut self,
        expr: &ExprValue,
        module_name: Option<&str>,
    ) -> Result<Value, VmError> {
        let head_name = expr.head.as_str();
        let head = ExprHead::from_name(head_name);
        let args = expr.args_snapshot();
        if let Some(head) = head {
            debug_assert_eq!(head.spec().runtime_eval, Self::runtime_eval_support(head));
        }

        match head {
            Some(ExprHead::Call) => {
                // Expr(:call, :func, args...)
                if args.is_empty() {
                    return Err(VmError::TypeError(
                        "call expression requires function name".to_string(),
                    ));
                }

                // Evaluate the remaining call arguments first; keyword
                // parameters are separated into a kwargs map for the dispatch
                // path below.
                let (eval_args, kwargs_map) = self.eval_call_arguments(&args[1..], module_name)?;

                // The callee may be a plain Symbol (`identity(5)`, `Val(3)`) or a
                // parametric `:curly` Expr such as `Val{3}()` whose head is a
                // Symbol naming a type with concrete type parameters (Issue #4976).
                match &args[0] {
                    Value::Symbol(s) => {
                        self.eval_call_with_kwargs(s.as_str(), eval_args, kwargs_map)
                    }
                    Value::GlobalRef(globalref) => {
                        if kwargs_map.is_empty()
                            && globalref.module == "Core"
                            && globalref.name.as_str() == "eval"
                        {
                            self.eval_core_eval(eval_args)
                        } else {
                            self.eval_globalref_call(
                                &globalref.module,
                                globalref.name.as_str(),
                                eval_args,
                                kwargs_map,
                            )
                        }
                    }
                    Value::Expr(callee) if ExprHead::is_expr(callee, ExprHead::Dot) => {
                        let (module_name, func_name) = Self::eval_dotted_callee_parts(callee)?;
                        if kwargs_map.is_empty() && module_name == "Core" && func_name == "eval" {
                            self.eval_core_eval(eval_args)
                        } else {
                            self.eval_globalref_call(
                                &module_name,
                                &func_name,
                                eval_args,
                                kwargs_map,
                            )
                        }
                    }
                    Value::Expr(callee)
                        if matches!(
                            ExprHead::from_expr(callee),
                            Some(ExprHead::Curly | ExprHead::ParametrizedTypeExpression)
                        ) =>
                    {
                        if !kwargs_map.is_empty() {
                            return Err(VmError::TypeError(
                                "eval: keyword arguments are not supported for parametric constructors"
                                    .to_string(),
                            ));
                        }
                        // Reconstruct the parametric type name, e.g. `Val{3}`, and
                        // construct the corresponding parametric struct instance
                        // (`Val{3}()`).
                        let type_name = self.eval_curly_type_name(callee)?;
                        self.eval_construct_parametric(&type_name, eval_args)
                    }
                    other => Err(VmError::TypeError(format!(
                        "call expression function must be Symbol or GlobalRef, got {:?}",
                        other.value_type()
                    ))),
                }
            }

            // Block: evaluate statements in sequence, return last
            Some(ExprHead::Block) => {
                let mut result = Value::Nothing;
                for arg in &args {
                    // Skip LineNumberNode
                    if matches!(arg, Value::LineNumberNode(_)) {
                        continue;
                    }
                    result = self.eval_expr_value_with_module(arg, module_name)?;
                }
                Ok(result)
            }

            // Let: evaluate bindings and body in an isolated clone of the current frame.
            Some(ExprHead::Let) => self.eval_let_expr(expr, module_name),

            // Try/catch/finally: Expr(:try, try_block, catch_var_or_false,
            // catch_block_or_false[, finally_block]).
            Some(ExprHead::Try) => self.eval_try_expr(&args, module_name),

            // Tuple literal: Expr(:tuple, args...)
            Some(ExprHead::Tuple) => {
                let mut elements = Vec::with_capacity(args.len());
                for arg in &args {
                    elements.push(self.eval_expr_value_with_module(arg, module_name)?);
                }
                Ok(Value::Tuple(TupleValue::new(elements)))
            }

            // Vector literal: Expr(:vect, args...)
            Some(ExprHead::Vect) => {
                let mut elements = Vec::with_capacity(args.len());
                for arg in &args {
                    elements.push(self.eval_expr_value_with_module(arg, module_name)?);
                }
                Ok(self.array_value_to_wrapper(ArrayValue::any_vector(elements))?)
            }

            // Parametric type expression: Expr(:curly, :Val, 3) => Val{3}
            Some(ExprHead::Curly | ExprHead::ParametrizedTypeExpression) => {
                let type_name = self.eval_curly_type_name(expr)?;
                Ok(Value::DataType(Box::new(JuliaType::from_name_or_struct(
                    &type_name,
                ))))
            }

            // String interpolation expression: Expr(:string, parts...)
            Some(ExprHead::String) => {
                let mut parts = Vec::with_capacity(args.len());
                for arg in &args {
                    parts.push(self.eval_expr_value_with_module(arg, module_name)?);
                }
                self.eval_call("string", parts)
            }

            // Conditional expression: Expr(:if/:elseif, cond, then[, else])
            Some(ExprHead::If | ExprHead::ElseIf) => {
                if !(2..=3).contains(&args.len()) {
                    return Err(VmError::TypeError(
                        "if/elseif expression requires 2 or 3 args".to_string(),
                    ));
                }

                let condition = self.eval_expr_value_with_module(&args[0], module_name)?;
                let branch = match condition {
                    Value::Bool(true) => Some(&args[1]),
                    Value::Bool(false) => args.get(2),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "if condition must be Bool, got {:?}",
                            other.value_type()
                        )))
                    }
                };

                match branch {
                    Some(branch_expr) => self.eval_expr_value_with_module(branch_expr, module_name),
                    None => Ok(Value::Nothing),
                }
            }

            // Comparison: ==, !=, <, >, <=, >=
            Some(ExprHead::Comparison) => {
                if args.len() < 3 || args.len().is_multiple_of(2) {
                    return Err(VmError::TypeError(
                        "comparison requires value/operator/value pairs".to_string(),
                    ));
                }
                let mut left = self.eval_expr_value_with_module(&args[0], module_name)?;
                for pair in args[1..].chunks_exact(2) {
                    let op = match &pair[0] {
                        Value::Symbol(s) => s.as_str().to_string(),
                        _ => {
                            return Err(VmError::TypeError(
                                "comparison operator must be Symbol".to_string(),
                            ))
                        }
                    };
                    let right = self.eval_expr_value_with_module(&pair[1], module_name)?;
                    if !matches!(
                        self.eval_comparison(&op, left, right.clone())?,
                        Value::Bool(true)
                    ) {
                        return Ok(Value::Bool(false));
                    }
                    left = right;
                }
                Ok(Value::Bool(true))
            }

            // && and ||
            Some(ExprHead::AndAnd) => {
                if args.len() != 2 {
                    return Err(VmError::TypeError("&& requires 2 args".to_string()));
                }
                let left = self.eval_expr_value_with_module(&args[0], module_name)?;
                if let Value::Bool(false) = left {
                    return Ok(Value::Bool(false));
                }
                self.eval_expr_value_with_module(&args[1], module_name)
            }

            Some(ExprHead::OrOr) => {
                if args.len() != 2 {
                    return Err(VmError::TypeError("|| requires 2 args".to_string()));
                }
                let left = self.eval_expr_value_with_module(&args[0], module_name)?;
                if let Value::Bool(true) = left {
                    return Ok(Value::Bool(true));
                }
                self.eval_expr_value_with_module(&args[1], module_name)
            }

            // Assignment: x = expr
            Some(ExprHead::Assign) => {
                if args.len() != 2 {
                    return Err(VmError::TypeError(
                        "assignment requires exactly 2 args".to_string(),
                    ));
                }

                // First arg is the variable name (as Symbol)
                let var_name = match &args[0] {
                    Value::Symbol(s) => s.as_str().to_string(),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "assignment target must be Symbol, got {:?}",
                            other.value_type()
                        )))
                    }
                };

                // Second arg is the value to assign
                let value = self.eval_expr_value_with_module(&args[1], module_name)?;

                if let Some(module_name) = module_name {
                    self.store_global_value(
                        &format!("{}.{}", module_name, var_name),
                        value.clone(),
                    );
                } else {
                    // Store the value in the current frame
                    self.set_variable_value(&var_name, value.clone());
                }

                // Return the assigned value (Julia semantics)
                Ok(value)
            }

            // Indexing: Expr(:ref, container, indices...) is getindex
            // (Issue #5932). Scoped to Array + Matrix; the eval call path is not
            // equivalent to the compiler's IndexLoad for Tuple/Dict. Evaluate the
            // container and each index, then delegate to the `getindex` builtin.
            Some(ExprHead::Ref) => {
                if args.is_empty() {
                    return Err(VmError::TypeError(
                        "ref expression requires a container".to_string(),
                    ));
                }
                let mut evaluated_args = Vec::with_capacity(args.len());
                for arg in &args {
                    evaluated_args.push(self.eval_expr_value_with_module(arg, module_name)?);
                }
                self.eval_call("getindex", evaluated_args)
            }

            // Quote: `eval(Expr(:quote, e))` returns the inner expression `e`
            // UNEVALUATED — one level of quoting is removed — matching upstream
            // Julia (`eval(Expr(:quote, e)) == e`). This is reached when an
            // eval'd expression contains a *nested* quote literal, e.g.
            // `eval(:(eval(:(h()))))`: the inner `:(h())` arrives here as
            // `Expr(:quote, Expr(:call, :h))` (Issue #5978). It mirrors the
            // `Value::QuoteNode` unwrap in `eval_expr_value`. A well-formed
            // `:quote` carries exactly one body argument. NOTE: `$`
            // interpolation inside such a nested quote is NOT resolved here (it
            // is applied by quote *lowering* at construction time); a body still
            // holding `Expr(:$, …)` is returned as data unchanged.
            Some(ExprHead::Quote) => args
                .first()
                .cloned()
                .ok_or_else(|| VmError::TypeError("quote expression requires a body".to_string())),

            // Copyast: evaluate and return its payload as AST/data.
            Some(ExprHead::CopyAst) => {
                if args.len() != 1 {
                    return Err(VmError::TypeError(
                        "copyast expression requires exactly 1 arg".to_string(),
                    ));
                }
                self.eval_expr_value_with_module(&args[0], module_name)
            }

            // Return: a generated body may return Expr(:return, value_expr).
            // In the returned-Expr compatibility evaluator this is the staged
            // result marker, so evaluate and return its payload.
            Some(ExprHead::Return) => {
                if args.len() != 1 {
                    return Err(VmError::TypeError(
                        "return expression requires exactly 1 arg".to_string(),
                    ));
                }
                self.eval_expr_value_with_module(&args[0], module_name)
            }

            _ => Err(VmError::NotImplemented(format!(
                "eval: unsupported Expr head '{}'",
                head_name
            ))),
        }
    }

    fn runtime_eval_support(head: ExprHead) -> bool {
        matches!(
            head,
            ExprHead::Call
                | ExprHead::Block
                | ExprHead::Let
                | ExprHead::Try
                | ExprHead::Tuple
                | ExprHead::Vect
                | ExprHead::Curly
                | ExprHead::ParametrizedTypeExpression
                | ExprHead::String
                | ExprHead::If
                | ExprHead::ElseIf
                | ExprHead::Comparison
                | ExprHead::AndAnd
                | ExprHead::OrOr
                | ExprHead::Assign
                | ExprHead::Ref
                | ExprHead::Quote
                | ExprHead::CopyAst
                | ExprHead::Return
        )
    }

    fn eval_try_expr(
        &mut self,
        args: &[Value],
        module_name: Option<&str>,
    ) -> Result<Value, VmError> {
        if args.len() < 3 {
            return Err(VmError::TypeError(
                "try expression requires try block, catch variable, and catch block".to_string(),
            ));
        }

        let finally_expr = if args.len() >= 5 || args.len() == 4 {
            args.get(3)
        } else {
            None
        }
        .filter(|expr| !matches!(expr, Value::Bool(false)));
        let else_expr = args
            .get(4)
            .filter(|expr| !matches!(expr, Value::Bool(false)));

        let try_result = self.eval_expr_value_with_module(&args[0], module_name);
        let mut result = match try_result {
            Ok(value) => match else_expr {
                Some(expr) => self.eval_expr_value_with_module(expr, module_name),
                None => Ok(value),
            },
            Err(err) => {
                if matches!(args.get(2), Some(Value::Bool(false))) {
                    Err(err)
                } else {
                    if let Some(Value::Symbol(name)) = args.get(1) {
                        self.set_variable_value(name.as_str(), Value::Str(err.to_string()));
                    }
                    self.eval_expr_value_with_module(&args[2], module_name)
                }
            }
        };

        if let Some(finally_expr) = finally_expr {
            if let Err(finally_err) = self.eval_expr_value_with_module(finally_expr, module_name) {
                result = Err(finally_err);
            }
        }

        result
    }

    fn eval_call_arguments(
        &mut self,
        args: &[Value],
        module_name: Option<&str>,
    ) -> Result<(Vec<Value>, HashMap<String, Value>), VmError> {
        let mut positional = Vec::new();
        let mut kwargs = HashMap::new();

        for (idx, arg) in args.iter().enumerate() {
            if let Value::Expr(parameters) = arg {
                if ExprHead::is_expr(parameters, ExprHead::Parameters) {
                    if idx != 0 {
                        return Err(VmError::TypeError(
                            "eval: parameters expression must precede positional arguments"
                                .to_string(),
                        ));
                    }
                    self.eval_call_parameters(parameters, &mut kwargs, module_name)?;
                    continue;
                }
                if ExprHead::is_expr(parameters, ExprHead::Kw) {
                    self.eval_call_keyword(parameters, &mut kwargs, module_name)?;
                    continue;
                }
            }
            positional.push(self.eval_expr_value_with_module(arg, module_name)?);
        }

        Ok((positional, kwargs))
    }

    fn eval_call_parameters(
        &mut self,
        parameters: &ExprValue,
        kwargs: &mut HashMap<String, Value>,
        module_name: Option<&str>,
    ) -> Result<(), VmError> {
        for param in parameters.args_snapshot() {
            let Value::Expr(kw) = param else {
                return Err(VmError::TypeError(
                    "eval: parameters entries must be keyword expressions".to_string(),
                ));
            };
            self.eval_call_keyword(&kw, kwargs, module_name)?;
        }
        Ok(())
    }

    fn eval_call_keyword(
        &mut self,
        kw: &ExprValue,
        kwargs: &mut HashMap<String, Value>,
        module_name: Option<&str>,
    ) -> Result<(), VmError> {
        let kw_args = kw.args_snapshot();
        if !ExprHead::is_expr(kw, ExprHead::Kw) || kw_args.len() != 2 {
            return Err(VmError::TypeError(
                "eval: keyword expression requires name and value".to_string(),
            ));
        }
        let name = match &kw_args[0] {
            Value::Symbol(s) => s.as_str().to_string(),
            other => {
                return Err(VmError::TypeError(format!(
                    "eval: keyword name must be Symbol, got {:?}",
                    other.value_type()
                )))
            }
        };
        let value = self.eval_expr_value_with_module(&kw_args[1], module_name)?;
        kwargs.insert(name, value);
        Ok(())
    }

    fn eval_let_expr(
        &mut self,
        expr: &ExprValue,
        module_name: Option<&str>,
    ) -> Result<Value, VmError> {
        let args = expr.args_snapshot();
        if args.is_empty() {
            return Err(VmError::TypeError(
                "let expression requires a body".to_string(),
            ));
        }

        let Some(scope_frame) = self.frames.last().cloned() else {
            return Err(VmError::InternalError(
                "let expression requires an evaluation frame".to_string(),
            ));
        };

        self.try_push_temporary_call_frame(scope_frame)?;
        let result = (|| {
            let (body, bindings) = args
                .split_last()
                .ok_or_else(|| VmError::TypeError("let expression requires a body".to_string()))?;
            for binding in bindings {
                self.eval_let_binding(binding, module_name)?;
            }
            self.eval_expr_value_with_module(body, module_name)
        })();
        self.pop_call_frame();
        result
    }

    fn eval_let_binding(
        &mut self,
        binding: &Value,
        module_name: Option<&str>,
    ) -> Result<(), VmError> {
        let Value::Expr(assign) = binding else {
            return Err(VmError::TypeError(
                "let binding must be an assignment expression".to_string(),
            ));
        };
        let assign_args = assign.args_snapshot();
        if !ExprHead::is_expr(assign, ExprHead::Assign) || assign_args.len() != 2 {
            return Err(VmError::TypeError(
                "let binding must be an assignment expression".to_string(),
            ));
        }

        let var_name = match &assign_args[0] {
            Value::Symbol(s) => s.as_str().to_string(),
            other => {
                return Err(VmError::TypeError(format!(
                    "let binding target must be Symbol, got {:?}",
                    other.value_type()
                )))
            }
        };
        let value = self.eval_expr_value_with_module(&assign_args[1], module_name)?;
        self.set_variable_value(&var_name, value);
        Ok(())
    }

    fn eval_core_eval(&mut self, args: Vec<Value>) -> Result<Value, VmError> {
        if args.len() != 2 {
            return Err(VmError::TypeError(
                "Core.eval requires exactly 2 arguments".to_string(),
            ));
        }

        let module_name = match &args[0] {
            Value::Module(module) => module.name.as_str(),
            Value::Symbol(symbol) => symbol.as_str(),
            other => {
                return Err(VmError::TypeError(format!(
                    "Core.eval first argument must be a Module, got {:?}",
                    other.value_type()
                )))
            }
        };

        let expr = Self::generated_eval_payload(&args[1]);
        self.eval_expr_value_with_module(&expr, Some(module_name))
    }

    /// Evaluate a function call from eval
    fn eval_call(&mut self, func: &str, args: Vec<Value>) -> Result<Value, VmError> {
        match func {
            // Arithmetic
            "+" => self.eval_binary_arith(&args, |a, b| a + b, |a, b| a + b),
            "-" => {
                if args.len() == 1 {
                    // Unary minus. Issue #4753: handle Int128 too — the
                    // literal parser now promotes overflowing decimal
                    // literals to Int128 (so `Meta.parse(repr(typemin(Int64)))`
                    // becomes `Expr(:call, :-, Int128(9223372036854775808))`).
                    // When -Int128(...) fits in Int64, narrow back so the
                    // result matches upstream's `typemin(Int64)` typing.
                    match &args[0] {
                        Value::I64(n) => Ok(Value::I64(n.wrapping_neg())),
                        Value::I128(n) => {
                            let negated = n.wrapping_neg();
                            if let Ok(narrow) = i64::try_from(negated) {
                                Ok(Value::I64(narrow))
                            } else {
                                Ok(Value::I128(negated))
                            }
                        }
                        Value::F64(n) => Ok(Value::F64(-n)),
                        Value::F32(n) => Ok(Value::F32(-n)),
                        _ => Err(VmError::TypeError(
                            "unary - requires numeric argument".to_string(),
                        )),
                    }
                } else {
                    self.eval_binary_arith(&args, |a, b| a - b, |a, b| a - b)
                }
            }
            "*" => self.eval_binary_arith(&args, |a, b| a * b, |a, b| a * b),
            "/" => {
                // Division always returns Float64 in Julia
                if args.len() != 2 {
                    return Err(VmError::TypeError("/ requires 2 arguments".to_string()));
                }
                let a = self.to_f64(&args[0])?;
                let b = self.to_f64(&args[1])?;
                Ok(Value::F64(a / b))
            }
            "÷" | "div" => {
                if args.len() != 2 {
                    return Err(VmError::TypeError("div requires 2 arguments".to_string()));
                }
                match (&args[0], &args[1]) {
                    (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a / b)),
                    _ => Err(VmError::TypeError(
                        "div requires integer arguments".to_string(),
                    )),
                }
            }
            "%" | "mod" => {
                if args.len() != 2 {
                    return Err(VmError::TypeError("mod requires 2 arguments".to_string()));
                }
                match (&args[0], &args[1]) {
                    (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a % b)),
                    (Value::F64(a), Value::F64(b)) => Ok(Value::F64(a % b)),
                    _ => Err(VmError::TypeError(
                        "mod requires numeric arguments".to_string(),
                    )),
                }
            }
            "^" => {
                if args.len() != 2 {
                    return Err(VmError::TypeError("^ requires 2 arguments".to_string()));
                }
                match (&args[0], &args[1]) {
                    (Value::I64(base), Value::I64(exp)) => {
                        if *exp >= 0 {
                            Ok(Value::I64(base.pow(*exp as u32)))
                        } else {
                            Ok(Value::F64((*base as f64).powi(*exp as i32)))
                        }
                    }
                    (Value::F64(base), Value::I64(exp)) => Ok(Value::F64(base.powi(*exp as i32))),
                    (Value::F64(base), Value::F64(exp)) => Ok(Value::F64(base.powf(*exp))),
                    (Value::I64(base), Value::F64(exp)) => {
                        Ok(Value::F64((*base as f64).powf(*exp)))
                    }
                    _ => Err(VmError::TypeError(
                        "^ requires numeric arguments".to_string(),
                    )),
                }
            }

            // Comparison
            "==" => self.eval_comparison("==", args[0].clone(), args[1].clone()),
            "!=" => self.eval_comparison("!=", args[0].clone(), args[1].clone()),
            "<" => self.eval_comparison("<", args[0].clone(), args[1].clone()),
            ">" => self.eval_comparison(">", args[0].clone(), args[1].clone()),
            "<=" => self.eval_comparison("<=", args[0].clone(), args[1].clone()),
            ">=" => self.eval_comparison(">=", args[0].clone(), args[1].clone()),

            // Math functions
            "sqrt" => {
                if args.len() != 1 {
                    return Err(VmError::TypeError("sqrt requires 1 argument".to_string()));
                }
                let x = self.to_f64(&args[0])?;
                Ok(Value::F64(x.sqrt()))
            }
            "abs" => {
                if args.len() != 1 {
                    return Err(VmError::TypeError("abs requires 1 argument".to_string()));
                }
                match &args[0] {
                    Value::I64(n) => Ok(Value::I64(n.abs())),
                    Value::F64(n) => Ok(Value::F64(n.abs())),
                    _ => Err(VmError::TypeError(
                        "abs requires numeric argument".to_string(),
                    )),
                }
            }
            "sin" => {
                if args.len() != 1 {
                    return Err(VmError::TypeError("sin requires 1 argument".to_string()));
                }
                let x = self.to_f64(&args[0])?;
                Ok(Value::F64(x.sin()))
            }
            "cos" => {
                if args.len() != 1 {
                    return Err(VmError::TypeError("cos requires 1 argument".to_string()));
                }
                let x = self.to_f64(&args[0])?;
                Ok(Value::F64(x.cos()))
            }

            // Boolean (handle both "!" and escaped "\!")
            "!" | "\\!" => {
                if args.len() != 1 {
                    return Err(VmError::TypeError("! requires 1 argument".to_string()));
                }
                match &args[0] {
                    Value::Bool(b) => Ok(Value::Bool(!b)),
                    _ => Err(VmError::TypeError(
                        "! requires boolean argument".to_string(),
                    )),
                }
            }

            // Anything the mini-interpreter does not special-case (user
            // functions, builtins like `identity`, constructors like `Val`,
            // …) is dispatched through the real VM call path (Issue #4976).
            _ => self.eval_dispatch_call(func, args),
        }
    }

    fn eval_call_with_kwargs(
        &mut self,
        func: &str,
        args: Vec<Value>,
        kwargs_map: HashMap<String, Value>,
    ) -> Result<Value, VmError> {
        if kwargs_map.is_empty() {
            self.eval_call(func, args)
        } else {
            self.eval_dispatch_call_with_kwargs(func, args, kwargs_map)
        }
    }

    fn eval_globalref_call(
        &mut self,
        module_name: &str,
        func_name: &str,
        args: Vec<Value>,
        kwargs_map: HashMap<String, Value>,
    ) -> Result<Value, VmError> {
        if kwargs_map.is_empty()
            && module_name == "Base"
            && matches!(func_name, "println" | "print" | "string")
        {
            return self.eval_call(func_name, args);
        }

        let qualified_name = format!("{}.{}", module_name, func_name);
        self.eval_call_with_kwargs(&qualified_name, args, kwargs_map)
    }

    fn eval_dotted_callee_parts(callee: &ExprValue) -> Result<(String, String), VmError> {
        let args = callee.args_snapshot();
        if args.len() != 2 {
            return Err(VmError::TypeError(
                "dotted call expression requires module and function name".to_string(),
            ));
        }

        let module_name = Self::eval_dotted_symbol_part(&args[0], "module")?;
        let func_name = match &args[1] {
            Value::QuoteNode(inner) => Self::eval_dotted_symbol_part(inner, "function")?,
            other => Self::eval_dotted_symbol_part(other, "function")?,
        };
        Ok((module_name, func_name))
    }

    fn eval_dotted_symbol_part(value: &Value, role: &str) -> Result<String, VmError> {
        match value {
            Value::Symbol(symbol) => Ok(symbol.as_str().to_string()),
            other => Err(VmError::TypeError(format!(
                "dotted call expression {role} must be Symbol, got {:?}",
                other.value_type()
            ))),
        }
    }

    /// Reconstruct a parametric type name (e.g. `Val{3}`) from a `:curly` Expr.
    ///
    /// The head argument must be a Symbol (the base type name); the remaining
    /// arguments are evaluated and rendered into `Base{P1, P2, ...}` form so the
    /// result can be resolved as a `DataType` by the normal call path.
    fn eval_curly_type_name(&mut self, curly: &ExprValue) -> Result<String, VmError> {
        let args = curly.args_snapshot();
        let base = match args.first() {
            Some(Value::Symbol(s)) => s.as_str().to_string(),
            _ => {
                return Err(VmError::TypeError(
                    "eval: curly expression base must be a Symbol".to_string(),
                ))
            }
        };
        let mut params = Vec::new();
        for arg in &args[1..] {
            let value = self.eval_expr_value(arg)?;
            params.push(self.eval_type_param_to_string(&value));
        }
        if params.is_empty() {
            Ok(base)
        } else {
            Ok(format!("{}{{{}}}", base, params.join(", ")))
        }
    }

    /// Construct a parametric struct instance such as `Val{3}()` whose callee is
    /// a `:curly`/`:parametrizedtypeexpression` Expr (Issue #4976).
    ///
    /// The base struct definition is looked up by stripping the type parameters
    /// from `type_name` (`Val{3}` -> `Val`); the constructed instance keeps the
    /// fully-parametrized name so `typeof`/`isa` report `Val{3}`.
    fn eval_construct_parametric(
        &mut self,
        type_name: &str,
        args: Vec<Value>,
    ) -> Result<Value, VmError> {
        let base_name = type_name.split('{').next().unwrap_or(type_name);
        // The struct may be registered either bare (`Val`) or with its
        // declared type parameters (`Val{Any}`); match on the base name.
        let found = self
            .struct_defs
            .iter()
            .enumerate()
            .find(|(_, def)| def.name.split('{').next().unwrap_or(&def.name) == base_name)
            .map(|(type_id, def)| (type_id, def.fields.len()));

        let Some((type_id, field_count)) = found else {
            return Err(VmError::TypeError(format!(
                "eval: parametric type '{}' not found",
                type_name
            )));
        };

        if field_count != args.len() {
            return Err(VmError::MethodError(format!(
                "no method matching {}({} args)",
                type_name,
                args.len()
            )));
        }

        let idx = self.struct_heap.len();
        self.struct_heap
            .push(crate::vm::value::StructInstance::with_name(
                type_id,
                type_name.to_string(),
                args,
            ));
        Ok(Value::StructRef(idx))
    }

    /// Render an evaluated type parameter into its textual form for parametric
    /// type names. Value parameters (e.g. the `3` in `Val{3}`) print as their
    /// literal; type parameters print as their type name.
    fn eval_type_param_to_string(&self, value: &Value) -> String {
        match value {
            Value::Symbol(s) => s.as_str().to_string(),
            Value::DataType(jt) => jt.name().to_string(),
            Value::I64(n) => n.to_string(),
            Value::I32(n) => n.to_string(),
            Value::I16(n) => n.to_string(),
            Value::I8(n) => n.to_string(),
            Value::U64(n) => n.to_string(),
            Value::U32(n) => n.to_string(),
            Value::U16(n) => n.to_string(),
            Value::U8(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Char(c) => format!("'{}'", c),
            Value::Str(s) => format!("\"{}\"", s),
            other => format!("{:?}", other),
        }
    }

    /// Dispatch a function/constructor call through the real VM call path.
    ///
    /// This reuses `call_runtime_callable_value`, which performs full multiple
    /// dispatch (user methods, builtins, intrinsics, parametric constructors,
    /// type-parameter binding). When the call starts a VM frame, we drive the
    /// main interpreter loop synchronously to completion and recover the return
    /// value (Issue #4976).
    ///
    /// When the call starts a VM frame, the nested interpreter loop is driven by
    /// `run_until_frame_return`, which stops exactly when that frame returns
    /// (rather than continuing into the caller's instruction stream). The outer
    /// `ip` is restored afterwards so the enclosing `eval` builtin resumes
    /// cleanly.
    pub(in crate::vm) fn eval_dispatch_call(
        &mut self,
        func: &str,
        args: Vec<Value>,
    ) -> Result<Value, VmError> {
        // Bound the nesting of `eval`-initiated VM dispatch so that an
        // `eval`-driven self-recursion fails safely with a StackOverflow
        // VmError rather than exhausting the native call stack and crashing the
        // process (Issue #5014). Each nested call drives `run_until_frame_return`,
        // which may itself re-enter the `eval` builtin and recurse here on the
        // Rust stack. `enter_eval_dispatch` errors out *before* incrementing when
        // the bound is hit, so we must not call `exit_eval_dispatch` on that path.
        self.enter_eval_dispatch()?;

        let func_val = Value::Function(FunctionValue::new(func.to_string()));

        // Frame depth before the call: the awaited frame, once pushed, sits at
        // `target_depth + 1`, and returning rewinds back to `target_depth`.
        let target_depth = self.frames.len();
        let saved_stack_len = self.stack.len();
        let saved_return_ips_len = self.return_ips.len();
        let saved_handlers_len = self.handlers.len();
        let saved_ip = self.ip;
        // Execution-state snapshot for the error-unwind path (Issue #7687).
        let unwind = self.eval_dispatch_unwind_snapshot();

        // Install the ancestor-handler floor (Issue #5972) for the *entire*
        // dispatch, not just the `StartedFrame` → `run_until_frame_return` arm.
        // The `Immediate` arm runs a builtin via `execute_runtime_builtin_immediate`,
        // whose `self.raise(err)?` would otherwise *catch* an ancestor handler
        // (the floor `run_until_frame_return` installs is already restored by the
        // time control returns there) and then re-surface it through
        // `RuntimeCallableResult::Raised` → `Err(pending_error)` below — a
        // double-handling that pops the handler AND clobbers `catch_ip` with
        // `saved_ip`, so the error escapes uncaught (Issue #5979, reached via a
        // doubly-nested `eval(:(eval(:(boom()))))`). Setting the floor here makes
        // that inner `self.raise` decline the ancestor handler and propagate `Err`
        // instead, so the outer `run()` loop's `CallBuiltin` arm re-`raise`s it
        // once at the correct level. Saved/restored so nested evals see their own
        // floor. `run_until_frame_return` re-asserts the same floor for its arm.
        let saved_floor = self.eval_dispatch_floor;
        self.eval_dispatch_floor = Some(target_depth);

        let result = match self.call_runtime_callable_value(func_val, args) {
            Ok(RuntimeCallableResult::Immediate(value)) => Ok(value),
            Ok(RuntimeCallableResult::StartedFrame) => self.run_until_frame_return(target_depth),
            Ok(RuntimeCallableResult::Raised) => Err(self
                .pending_error
                .take()
                .unwrap_or_else(|| VmError::InternalError("eval: call raised".to_string()))),
            Err(err) => Err(err),
        };

        self.discard_eval_dispatch_state(
            target_depth,
            saved_stack_len,
            saved_return_ips_len,
            saved_handlers_len,
        );

        // Restore the enclosing dispatch's floor (or `None` at the top level).
        self.eval_dispatch_floor = saved_floor;

        // Restore the outer instruction pointer so the eval builtin resumes
        // where it left off.
        self.ip = saved_ip;

        // When the call raised, the error surfaced as a Rust `Err` instead of
        // unwinding through a bytecode exception handler, so the failing
        // callee's frame(s), operand-stack residue, return addresses, and any
        // try-handlers it installed are still live. Restore the pre-call depths
        // so the eval caller (e.g. an `eval`-driven `try`/`catch`) continues in
        // its original frame; otherwise a later `StoreSlot` writes the stale
        // callee frame's slot table (`StoreSlot: slot out of bounds`, #7687).
        if result.is_err() {
            self.restore_eval_dispatch_unwind(&unwind);
        }

        // Always leave the dispatch level we entered above, regardless of
        // success / error, so the depth counter never leaks (Issue #5014).
        self.exit_eval_dispatch();

        result
    }

    /// Snapshot of the VM execution-state depths taken before an `eval`-driven
    /// dispatch, used to unwind cleanly when the call raises (Issue #7687).
    fn eval_dispatch_unwind_snapshot(&self) -> EvalDispatchUnwind {
        EvalDispatchUnwind {
            frame_depth: self.frames.len(),
            stack_depth: self.stack.len(),
            return_ip_depth: self.return_ips.len(),
            handler_depth: self.handlers.len(),
        }
    }

    /// Restore the VM frame/operand-stack/return-address/handler depths to a
    /// pre-dispatch snapshot after an `eval`-driven call raised (Issue #7687).
    ///
    /// An error escaping an eval dispatch propagates as a Rust `Err` rather than
    /// being unwound by a bytecode exception handler, so the failing callee
    /// leaves its frame(s), stack residue, return addresses, and installed
    /// try-handlers behind. Truncating each back to its snapshot lets the eval
    /// caller resume in its original frame.
    fn restore_eval_dispatch_unwind(&mut self, snapshot: &EvalDispatchUnwind) {
        if self.frames.len() > snapshot.frame_depth {
            self.frames.truncate(snapshot.frame_depth);
            self.generated_expr_pending_keys
                .retain(|depth, _| *depth < snapshot.frame_depth);
            self.generated_expr_pending_eval_frames
                .retain(|depth, _| *depth < snapshot.frame_depth);
        }
        if self.stack.len() > snapshot.stack_depth {
            self.stack.truncate(snapshot.stack_depth);
        }
        if self.return_ips.len() > snapshot.return_ip_depth {
            self.return_ips.truncate(snapshot.return_ip_depth);
        }
        if self.handlers.len() > snapshot.handler_depth {
            self.handlers.truncate(snapshot.handler_depth);
        }
    }

    fn eval_dispatch_call_with_kwargs(
        &mut self,
        func: &str,
        args: Vec<Value>,
        kwargs_map: HashMap<String, Value>,
    ) -> Result<Value, VmError> {
        self.enter_eval_dispatch()?;

        let func_val = Value::Function(FunctionValue::new(func.to_string()));
        let declared_arg_type_names = self.callable_dispatch_type_names(&args);
        let target_depth = self.frames.len();
        let saved_stack_len = self.stack.len();
        let saved_return_ips_len = self.return_ips.len();
        let saved_handlers_len = self.handlers.len();
        let saved_ip = self.ip;
        // Execution-state snapshot for the error-unwind path (Issue #7687).
        let unwind = self.eval_dispatch_unwind_snapshot();
        let saved_floor = self.eval_dispatch_floor;
        self.eval_dispatch_floor = Some(target_depth);

        let result = match self.invoke_runtime_callable_value_with_signature_and_kwargs(
            func_val,
            args,
            &declared_arg_type_names,
            &kwargs_map,
        ) {
            Ok(RuntimeCallableResult::Immediate(value)) => Ok(value),
            Ok(RuntimeCallableResult::StartedFrame) => self.run_until_frame_return(target_depth),
            Ok(RuntimeCallableResult::Raised) => Err(self
                .pending_error
                .take()
                .unwrap_or_else(|| VmError::InternalError("eval: call raised".to_string()))),
            Err(err) => Err(err),
        };

        self.discard_eval_dispatch_state(
            target_depth,
            saved_stack_len,
            saved_return_ips_len,
            saved_handlers_len,
        );

        self.eval_dispatch_floor = saved_floor;
        self.ip = saved_ip;
        // Unwind leftover callee state on the error path (Issue #7687); see
        // `eval_dispatch_call` for the rationale.
        if result.is_err() {
            self.restore_eval_dispatch_unwind(&unwind);
        }
        self.exit_eval_dispatch();

        result
    }

    fn discard_eval_dispatch_state(
        &mut self,
        target_depth: usize,
        saved_stack_len: usize,
        saved_return_ips_len: usize,
        saved_handlers_len: usize,
    ) {
        self.return_ips.truncate(saved_return_ips_len);
        self.handlers.truncate(saved_handlers_len);
        if self.frames.len() > target_depth {
            self.stack.truncate(saved_stack_len);
        }
        while self.frames.len() > target_depth {
            let depth = self.frames.len().saturating_sub(1);
            self.generated_expr_pending_keys.remove(&depth);
            self.generated_expr_pending_eval_frames.remove(&depth);
            if let Some(mut frame) = self.frames.pop() {
                if self.frame_pool.len() < Self::MAX_POOLED_FRAMES {
                    frame.clear_for_pool();
                    self.frame_pool.push(frame);
                }
            }
        }
    }

    /// Helper for binary arithmetic operations
    fn eval_binary_arith(
        &self,
        args: &[Value],
        int_op: fn(i64, i64) -> i64,
        float_op: fn(f64, f64) -> f64,
    ) -> Result<Value, VmError> {
        if args.len() != 2 {
            return Err(VmError::TypeError(
                "binary operation requires 2 arguments".to_string(),
            ));
        }
        match (&args[0], &args[1]) {
            (Value::I64(a), Value::I64(b)) => Ok(Value::I64(int_op(*a, *b))),
            (Value::F64(a), Value::F64(b)) => Ok(Value::F64(float_op(*a, *b))),
            (Value::I64(a), Value::F64(b)) => Ok(Value::F64(float_op(*a as f64, *b))),
            (Value::F64(a), Value::I64(b)) => Ok(Value::F64(float_op(*a, *b as f64))),
            _ => Err(VmError::TypeError(
                "arithmetic requires numeric arguments".to_string(),
            )),
        }
    }

    /// Helper for comparison operations
    pub(super) fn eval_comparison(
        &self,
        op: &str,
        left: Value,
        right: Value,
    ) -> Result<Value, VmError> {
        let result = match (left, right) {
            (Value::I64(a), Value::I64(b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                ">=" => a >= b,
                _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
            },
            (Value::F64(a), Value::F64(b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                ">=" => a >= b,
                _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
            },
            (Value::I64(a), Value::F64(b)) => {
                let a = a as f64;
                match op {
                    "==" => a == b,
                    "!=" => a != b,
                    "<" => a < b,
                    ">" => a > b,
                    "<=" => a <= b,
                    ">=" => a >= b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::F64(a), Value::I64(b)) => {
                let b = b as f64;
                match op {
                    "==" => a == b,
                    "!=" => a != b,
                    "<" => a < b,
                    ">" => a > b,
                    "<=" => a <= b,
                    ">=" => a >= b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            // Int128 comparisons
            (Value::I128(a), Value::I128(b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                ">=" => a >= b,
                _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
            },
            (Value::I128(a), Value::I64(b)) => {
                let b = b as i128;
                match op {
                    "==" => a == b,
                    "!=" => a != b,
                    "<" => a < b,
                    ">" => a > b,
                    "<=" => a <= b,
                    ">=" => a >= b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::I64(a), Value::I128(b)) => {
                let a = a as i128;
                match op {
                    "==" => a == b,
                    "!=" => a != b,
                    "<" => a < b,
                    ">" => a > b,
                    "<=" => a <= b,
                    ">=" => a >= b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            // BigInt comparisons
            (Value::BigInt(ref a), Value::BigInt(ref b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                ">=" => a >= b,
                _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
            },
            (Value::BigInt(ref a), Value::I64(b)) => {
                let b = num_bigint::BigInt::from(b);
                match op {
                    "==" => a.as_inner() == &b,
                    "!=" => a.as_inner() != &b,
                    "<" => a.as_inner() < &b,
                    ">" => a.as_inner() > &b,
                    "<=" => a.as_inner() <= &b,
                    ">=" => a.as_inner() >= &b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::I64(a), Value::BigInt(ref b)) => {
                let a = num_bigint::BigInt::from(a);
                match op {
                    "==" => &a == b.as_inner(),
                    "!=" => &a != b.as_inner(),
                    "<" => &a < b.as_inner(),
                    ">" => &a > b.as_inner(),
                    "<=" => &a <= b.as_inner(),
                    ">=" => &a >= b.as_inner(),
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::BigInt(ref a), Value::I128(b)) => {
                let b = num_bigint::BigInt::from(b);
                match op {
                    "==" => a.as_inner() == &b,
                    "!=" => a.as_inner() != &b,
                    "<" => a.as_inner() < &b,
                    ">" => a.as_inner() > &b,
                    "<=" => a.as_inner() <= &b,
                    ">=" => a.as_inner() >= &b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::I128(a), Value::BigInt(ref b)) => {
                let a = num_bigint::BigInt::from(a);
                match op {
                    "==" => &a == b.as_inner(),
                    "!=" => &a != b.as_inner(),
                    "<" => &a < b.as_inner(),
                    ">" => &a > b.as_inner(),
                    "<=" => &a <= b.as_inner(),
                    ">=" => &a >= b.as_inner(),
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::Bool(a), Value::Bool(b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                _ => {
                    return Err(VmError::TypeError(
                        "comparison not supported for Bool".to_string(),
                    ))
                }
            },
            (Value::Str(a), Value::Str(b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                ">=" => a >= b,
                _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
            },
            // DataType (Type) comparison - uses identity semantics like Julia
            (Value::DataType(a), Value::DataType(b)) => match op {
                "==" => type_objects_equal(&a, &b),
                "!=" => !type_objects_equal(&a, &b),
                _ => {
                    return Err(VmError::TypeError(format!(
                        "comparison op {} not supported for DataType",
                        op
                    )))
                }
            },
            _ => return Err(VmError::TypeError("comparison type mismatch".to_string())),
        };
        Ok(Value::Bool(result))
    }

    /// Convert Value to f64 for math operations
    pub(super) fn to_f64(&self, val: &Value) -> Result<f64, VmError> {
        match val {
            Value::I64(n) => Ok(*n as f64),
            Value::F64(n) => Ok(*n),
            Value::I32(n) => Ok(*n as f64),
            Value::F32(n) => Ok(*n as f64),
            _ => Err(VmError::TypeError("expected numeric value".to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::compile_and_run_value;

    /// Unbounded self-recursive `eval` must fail safely with a Stack overflow
    /// VmError rather than crashing the host process by exhausting the native
    /// call stack (Issue #5014). `eval_dispatch_call` recurses on the Rust stack
    /// for every nested VM call started from the `eval` builtin, so an
    /// `eval`-driven self-recursion would otherwise segfault.
    #[test]
    fn eval_self_recursion_fails_with_stack_overflow() {
        let src = r#"
            f() = eval(Meta.parse("f()"))
            f()
        "#;
        let result = compile_and_run_value(src, 0);
        let err = result.expect_err("unbounded eval self-recursion must return an error");
        assert!(
            err.contains("Stack overflow"),
            "expected a Stack overflow runtime error, got: {err}"
        );
    }

    /// A bounded `eval`-driven dispatch into a user function must still
    /// succeed, so the depth guard does not regress ordinary `eval` usage
    /// (`eval(Meta.parse("g(0)"))` recurses one level through the VM call path).
    #[test]
    fn eval_bounded_dispatch_succeeds() {
        let src = r#"
            function g(n)
                if n <= 0
                    return 42
                end
                return eval(Meta.parse("g(0)")) + n
            end
            g(5)
        "#;
        let result = compile_and_run_value(src, 0);
        let value = result.expect("bounded eval dispatch should succeed");
        assert!(
            matches!(value, crate::vm::Value::I64(47)),
            "g(5) = eval(g(0)) + 5 = 42 + 5 should yield 47, got: {value:?}"
        );
    }

    /// The classic arithmetic `eval` path (no VM dispatch) must remain
    /// unaffected by the depth guard (Issue #5014 regression guard).
    #[test]
    fn eval_arithmetic_still_works() {
        let result = compile_and_run_value(r#"eval(Meta.parse("1 + 1"))"#, 0);
        let value = result.expect("arithmetic eval should succeed");
        assert!(
            matches!(value, crate::vm::Value::I64(2)),
            "eval of 1 + 1 should yield 2, got: {value:?}"
        );
    }
}
