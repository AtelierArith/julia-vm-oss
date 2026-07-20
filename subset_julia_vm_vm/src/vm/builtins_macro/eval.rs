//! Expression evaluation for the macro system.
//!
//! Handles eval() builtin: evaluates Expr AST nodes at runtime.

// SAFETY: i64→u32 cast in integer exponentiation is guarded by `if *exp >= 0` check.
#![allow(clippy::cast_sign_loss)]

use crate::vm::splat::KwargsMap;
use std::collections::HashMap;

use crate::{builtins::BuiltinId, expr_heads::ExprHead, rng::RngLike, types::JuliaType};

use super::super::error::VmError;
use super::super::frame::Frame;
use super::super::hof_exec::state::RuntimeCallableResult;
use super::super::type_utils::type_objects_equal;
use super::super::types::{FunctionInfo, StructDefInfo};
use super::super::util::bind_value_to_slot;
use super::super::value::{ArrayValue, ExprValue, FunctionValue, TupleValue, Value, ValueType};
use super::super::{EvalDefinedMethod, Instr, ReplDefinitionActivation, Vm};

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

fn eval_struct_field_value_type(name: &str) -> ValueType {
    match name {
        "Int8" => ValueType::I8,
        "Int16" => ValueType::I16,
        "Int32" => ValueType::I32,
        "Int" | "Int64" => ValueType::I64,
        "Int128" => ValueType::I128,
        "UInt8" => ValueType::U8,
        "UInt16" => ValueType::U16,
        "UInt32" => ValueType::U32,
        "UInt" | "UInt64" => ValueType::U64,
        "UInt128" => ValueType::U128,
        "Float16" => ValueType::F16,
        "Float32" => ValueType::F32,
        "Float64" => ValueType::F64,
        "Bool" => ValueType::Bool,
        "String" | "AbstractString" => ValueType::Str,
        "Char" => ValueType::Char,
        _ => ValueType::Any,
    }
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
            self.enforce_generated_expr_cache_limit();
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

    /// Enter Julia's module-global `eval` boundary. Frames already active at
    /// entry belong to the compiled caller and are not lexical parents of the
    /// evaluated expression. Recursive tree walking keeps this floor, while a
    /// genuinely nested `eval` pushes a new one (Issue #11071).
    pub(super) fn eval_module_expr_value(
        &mut self,
        val: &Value,
        module_name: Option<&str>,
    ) -> Result<Value, VmError> {
        self.module_eval_scope_floors.push(self.frames.len());
        let result = self.eval_expr_value_with_module(val, module_name);
        self.module_eval_scope_floors.pop();
        result
    }

    pub(super) fn eval_expr_value_with_module(
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
            Value::Str(s) => Ok(Value::str_new(s.clone())),
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
                if s.as_str() == ":" && self.get_eval_variable_value(":").is_none() {
                    return Ok(Value::SliceAll);
                }
                if let Some(module_name) = module_name {
                    if !self.module_eval_scope_floors.is_empty() {
                        return self
                            .get_module_eval_variable_value(module_name, s.as_str())
                            .ok_or_else(|| VmError::UndefVarError(s.as_str().to_string()));
                    }
                    let qualified = format!("{}.{}", module_name, s.as_str());
                    if let Some(val) = self.get_eval_variable_value(&qualified) {
                        return Ok(val);
                    }
                }
                // Try to resolve the symbol to a variable value
                if let Some(val) = self.get_eval_variable_value(s.as_str()) {
                    Ok(val)
                } else {
                    Err(VmError::UndefVarError(s.as_str().to_string()))
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

        // `Meta.parse`'s compatibility converter historically exposed parser
        // node names for struct definitions. Accept those shapes directly in
        // runtime eval/include and install the type into the live VM (Issue
        // #10329). Normalized `Expr(:struct, mutable, header, body)` support can
        // share this installer once the parser converter is unified.
        if matches!(head_name, "structdefinition" | "mutablestructdefinition") {
            return self.eval_define_struct_from_parser_expr(
                &args,
                head_name == "mutablestructdefinition",
                module_name,
            );
        }
        if head_name == "fieldexpression" {
            if args.len() != 2 {
                return Err(VmError::TypeError(
                    "eval field expression requires object and field".to_string(),
                ));
            }
            let object = self.eval_expr_value_with_module(&args[0], module_name)?;
            let Value::Symbol(field_name) = &args[1] else {
                return Err(VmError::TypeError(
                    "eval field name must be a Symbol".to_string(),
                ));
            };
            let Value::StructRef(index) = object else {
                return Err(VmError::TypeError(
                    "eval field access requires a struct value".to_string(),
                ));
            };
            let instance = self.struct_heap.get(index).ok_or_else(|| {
                VmError::InternalError("eval field access has invalid struct reference".to_string())
            })?;
            let def = self.struct_defs.get(instance.type_id).ok_or_else(|| {
                VmError::InternalError("eval field access has invalid type id".to_string())
            })?;
            let field_index = def
                .fields
                .iter()
                .position(|(name, _)| name == field_name.as_str())
                .ok_or_else(|| {
                    VmError::TypeError(format!(
                        "type {} has no field {}",
                        def.name,
                        field_name.as_str()
                    ))
                })?;
            return instance
                .values
                .get(field_index)
                .cloned()
                .ok_or_else(|| VmError::InternalError("eval field value is missing".to_string()));
        }
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
            //
            // Short-form method definitions (`f(x) = expr`) are ALSO parsed
            // to `Expr(:(=), call_or_where, body)` — upstream Julia's
            // `Meta.lower` recognizes a call-shaped (optionally `where`-
            // wrapped) assignment target as a method definition rather than
            // a variable binding. `eval(:(f(x) = 100))` therefore reaches
            // this same arm; route it to `eval_define_function_from_expr`
            // instead of requiring a bare Symbol (Issue #8647).
            Some(ExprHead::Assign) => {
                if args.len() != 2 {
                    return Err(VmError::TypeError(
                        "assignment requires exactly 2 args".to_string(),
                    ));
                }

                if is_function_def_target(&args[0]) {
                    return self.eval_define_function_from_expr(
                        &args[0],
                        args[1].clone(),
                        module_name,
                    );
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

                self.set_eval_variable_value(&var_name, value.clone(), module_name);

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

            Some(ExprHead::Struct) => {
                self.eval_define_struct_from_parser_expr(&args, false, module_name)
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

            // `using` statements are compile-time import declarations. In the
            // include/eval path, package/module loading has already happened
            // during lowering, so evaluating the statement itself is a no-op.
            Some(ExprHead::UsingStatement) => Ok(Value::Nothing),

            // Field access `Expr(:., obj, QuoteNode(:name))` desugars to
            // `getfield(obj, :name)` — Julia's own lowering shape
            // (Issue #10525). Module-qualified VALUE reads whose first part
            // is an unresolvable module symbol stay Issue #11073 scope: the
            // object evaluation propagates its UndefVarError.
            Some(ExprHead::Dot) => {
                if args.len() != 2 {
                    return Err(VmError::TypeError(
                        "eval: field access expression requires object and field name".to_string(),
                    ));
                }
                let field = match &args[1] {
                    Value::QuoteNode(inner) => match inner.as_ref() {
                        Value::Symbol(sym) => sym.clone(),
                        other => {
                            return Err(VmError::TypeError(format!(
                                "eval: field name must be Symbol, got {:?}",
                                other.value_type()
                            )))
                        }
                    },
                    Value::Symbol(sym) => sym.clone(),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "eval: field name must be Symbol, got {:?}",
                            other.value_type()
                        )))
                    }
                };
                let object = self.eval_expr_value_with_module(&args[0], module_name)?;
                self.eval_dispatch_call("getfield", vec![object, Value::Symbol(field)])
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
                | ExprHead::UsingStatement
                | ExprHead::Struct
        )
    }

    fn eval_define_struct_from_parser_expr(
        &mut self,
        args: &[Value],
        is_mutable: bool,
        module_name: Option<&str>,
    ) -> Result<Value, VmError> {
        let (is_mutable, name_value, body_values) = match args.first() {
            Some(Value::Bool(flag)) if args.len() >= 3 => (*flag, &args[1], &args[2..]),
            Some(_) => (is_mutable, &args[0], &args[1..]),
            None => {
                return Err(VmError::TypeError(
                    "eval struct definition requires a name".to_string(),
                ))
            }
        };
        let Value::Symbol(name) = name_value else {
            return Err(VmError::TypeError(
                "eval struct definition requires a Symbol name".to_string(),
            ));
        };
        let local_name = name.as_str().to_string();
        let qualified_name = module_name
            .map(|module| format!("{module}.{local_name}"))
            .unwrap_or_else(|| local_name.clone());
        let already_installed = self
            .struct_defs
            .iter()
            .any(|def| def.name == qualified_name || def.name == local_name);
        let binding_name = module_name
            .map(|module| format!("{module}.{local_name}"))
            .unwrap_or_else(|| local_name.clone());
        if already_installed && self.get_variable_value(&binding_name).is_some() {
            return Err(VmError::TypeError(format!(
                "invalid redefinition of constant {local_name}"
            )));
        }

        let mut fields = Vec::new();
        let mut field_julia_types = Vec::new();
        let mut field_values = Vec::new();
        for value in body_values {
            if let Value::Expr(block) = value {
                if block.head.as_str() == "block" {
                    field_values.extend(block.args_snapshot());
                    continue;
                }
            }
            field_values.push(value.clone());
        }
        for field in &field_values {
            match field {
                Value::LineNumberNode(_) => {}
                Value::Symbol(field_name) => {
                    fields.push((field_name.as_str().to_string(), ValueType::Any));
                    field_julia_types.push(JuliaType::Any);
                }
                Value::Expr(field_expr)
                    if matches!(field_expr.head.as_str(), "typedexpression" | "::") =>
                {
                    let field_args = field_expr.args_snapshot();
                    if field_args.len() != 2 {
                        return Err(VmError::TypeError(
                            "eval struct field annotation requires name and type".to_string(),
                        ));
                    }
                    let Value::Symbol(field_name) = &field_args[0] else {
                        return Err(VmError::TypeError(
                            "eval struct field name must be a Symbol".to_string(),
                        ));
                    };
                    let field_type_name = match &field_args[1] {
                        Value::Symbol(ty) => ty.as_str().to_string(),
                        Value::DataType(ty) => ty.to_string(),
                        other => {
                            return Err(VmError::TypeError(format!(
                                "eval struct field type must be a type name, got {:?}",
                                other.value_type()
                            )))
                        }
                    };
                    let julia_type = JuliaType::from_name_or_struct(&field_type_name);
                    fields.push((
                        field_name.as_str().to_string(),
                        eval_struct_field_value_type(&field_type_name),
                    ));
                    field_julia_types.push(julia_type);
                }
                other => {
                    return Err(VmError::NotImplemented(format!(
                        "eval struct body item is not supported: {:?}",
                        other.value_type()
                    )))
                }
            }
        }

        if !already_installed {
            let type_id = self.install_runtime_struct_definition(StructDefInfo {
                name: qualified_name.clone(),
                is_mutable,
                fields,
                field_julia_types,
                parent_type: None,
            })?;
            self.repl_definition_activations
                .push(ReplDefinitionActivation::Struct(type_id));
        }
        self.eval_defined_struct_names
            .insert(qualified_name.clone());
        let type_value = Value::DataType(Box::new(JuliaType::Struct(qualified_name.clone())));
        if let Some(module) = module_name {
            self.store_global_value(&format!("{module}.{local_name}"), type_value.clone());
        } else {
            self.store_global_value(&local_name, type_value.clone());
        }
        Ok(type_value)
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
                    // This tree-walked catch consumes the raised VM exception.
                    // Clear the pending transport state so a later, unrelated
                    // error after the catch is not replaced by this stale one
                    // (Issue #11075).
                    self.pending_error = None;
                    self.pending_exception_value = None;
                    self.pending_backtrace = None;
                    // Every catch owns a fresh frame. Module eval already has a
                    // floor excluding its compiled caller. Generated/eval-
                    // defined walking creates a lexical floor that includes its
                    // legitimate current function frame but excludes callers
                    // below it (Issues #11071/#11075).
                    let lexical_floor = (self.module_eval_scope_floors.is_empty()
                        && self.lexical_eval_scope_floors.is_empty())
                    .then(|| self.frames.len().saturating_sub(1));
                    self.try_push_temporary_call_frame(Frame::new())?;
                    if let Some(floor) = lexical_floor {
                        self.lexical_eval_scope_floors.push(floor);
                    }
                    if let Some(Value::Symbol(name)) = args.get(1) {
                        self.set_variable_value(name.as_str(), Value::str_new(err.to_string()));
                    }
                    let catch_result = self.eval_expr_value_with_module(&args[2], module_name);
                    self.pop_call_frame();
                    if lexical_floor.is_some() {
                        self.lexical_eval_scope_floors.pop();
                    }
                    catch_result
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
    ) -> Result<(Vec<Value>, KwargsMap<Value>), VmError> {
        let mut positional = Vec::new();
        let mut kwargs = KwargsMap::new();

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
        kwargs: &mut KwargsMap<Value>,
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
        kwargs: &mut KwargsMap<Value>,
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

        // Every let owns a fresh frame. For generated/eval-defined walking, a
        // lexical floor includes the current argument frame so parameters and
        // enclosing assignments remain visible without admitting dynamic
        // caller scope (Issues #11071/#11075).
        let lexical_floor = (self.module_eval_scope_floors.is_empty()
            && self.lexical_eval_scope_floors.is_empty())
        .then(|| self.frames.len().saturating_sub(1));
        self.try_push_temporary_call_frame(Frame::new())?;
        if let Some(floor) = lexical_floor {
            self.lexical_eval_scope_floors.push(floor);
        }
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
        if lexical_floor.is_some() {
            self.lexical_eval_scope_floors.pop();
        }
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
        self.eval_module_expr_value(&expr, Some(module_name))
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
                    // checked_div: covers b == 0 and the i64::MIN / -1 overflow
                    // (DivideError in Julia, Issue #9429).
                    (Value::I64(a), Value::I64(b)) => a
                        .checked_div(*b)
                        .map(Value::I64)
                        .ok_or(VmError::DivisionByZero),
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
                    (Value::I64(a), Value::I64(b)) => {
                        if *b == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        // wrapping_rem: rem(typemin(Int64), -1) == 0 in Julia
                        // (Issue #9429).
                        Ok(Value::I64(a.wrapping_rem(*b)))
                    }
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
            _ if self.eval_defined_struct_names.iter().any(|name| {
                name.split('{').next().unwrap_or(name) == func.split('{').next().unwrap_or(func)
            }) =>
            {
                self.eval_construct_parametric(func, args)
            }
            _ => self.eval_dispatch_call(func, args),
        }
    }

    fn eval_call_with_kwargs(
        &mut self,
        func: &str,
        args: Vec<Value>,
        kwargs_map: KwargsMap<Value>,
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
        kwargs_map: KwargsMap<Value>,
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
            // A bare symbol in type-parameter position is also type syntax
            // (`Vector{Int}`), not necessarily a runtime variable read. Resolve
            // an actual eval-local/global binding when present, otherwise keep
            // the symbol for `eval_type_param_to_string` to normalize (#5930,
            // regression guard for the module-eval UndefVarError fix #11071).
            let value = match arg {
                Value::Symbol(symbol) => self
                    .get_eval_variable_value(symbol.as_str())
                    .unwrap_or_else(|| Value::Symbol(symbol.clone())),
                _ => self.eval_expr_value(arg)?,
            };
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

        // A bare struct name with no function methods is a constructor call
        // (Issue #10525): route it as the type-object callable so
        // `call_runtime_callable_value`'s default-DataType construction
        // fires (`eval(:(Foo(1)))`). Outer constructor methods, when they
        // exist, keep the ordinary function route below and still win.
        let func_val = if self.get_function_indices_by_name(func).is_empty()
            && self
                .struct_defs
                .iter()
                .any(|def| def.name == func || def.name.split('{').next() == Some(func))
        {
            Value::DataType(Box::new(crate::types::JuliaType::from_name_or_struct(func)))
        } else {
            Value::Function(FunctionValue::new(func.to_string()))
        };

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
        kwargs_map: KwargsMap<Value>,
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
        self.unwind_driven_callable_state(target_depth);
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

    /// Whether a signature-annotation name resolves to something bound at the
    /// point the runtime-`eval`'d definition executes (Issue #11146).
    ///
    /// Deliberately permissive — this predicate decides only WHICH error a
    /// typed-parameter definition raises, never whether it succeeds
    /// (`parse_call_signature` still defers every typed parameter). A name that
    /// resolves keeps the existing "typed parameters are deferred" gap; a name
    /// that does not resolve is a genuine forward/undefined reference and gets
    /// upstream's `UndefVarError`. No currently-working program changes
    /// behaviour: both outcomes were already errors.
    fn eval_signature_name_is_bound(&self, name: &str, module_name: Option<&str>) -> bool {
        // A builtin/static type (`Int`, `Float64`, `String`, ...).
        if JuliaType::from_name(name).is_some() {
            return true;
        }
        // A user struct or abstract type. Types declared inside `module M` are
        // registered under their QUALIFIED name (`M.Local`), so a definition
        // eval'd in M's own scope must be matched against `M.Local` as well as
        // the bare name — otherwise an already-defined module-local type is
        // rejected as a forward reference (found by an adversarial `codex exec`
        // review of this diff; `M.define_it()` in the fixture below).
        // The parametric-base forms (`Box` for `Box{Int}`) are matched too.
        let candidates: [Option<String>; 2] = [
            Some(name.to_string()),
            module_name.map(|m| format!("{m}.{name}")),
        ];
        for candidate in candidates.iter().flatten() {
            if self
                .struct_defs
                .iter()
                .any(|d| type_registration_matches(&d.name, candidate))
                || self
                    .abstract_types
                    .iter()
                    .any(|a| type_registration_matches(&a.name, candidate))
            {
                return true;
            }
        }
        // A binding visible in the target module's scope (a global bound to a
        // type object, a type alias, ...).
        if let Some(module_name) = module_name {
            if self
                .get_module_eval_variable_value(module_name, name)
                .is_some()
            {
                return true;
            }
        }
        // Anything else visible as a runtime binding (an enclosing `where`
        // binding, a Main global bound to a type object, ...).
        self.get_eval_variable_value(name).is_some()
    }

    /// Raise `UndefVarError` for the first signature annotation / `where` bound
    /// of a runtime-`eval`'d method definition that is not bound yet — upstream
    /// Julia's eager signature evaluation (Issue #11146; the runtime-path
    /// sibling of `compile/stmt.rs::emit_signature_definition_probes`).
    fn probe_eval_signature_annotations(
        &mut self,
        signature_expr: &Value,
        module_name: Option<&str>,
    ) -> Result<(), VmError> {
        for name in eval_signature_annotation_names(signature_expr) {
            if !self.eval_signature_name_is_bound(&name, module_name) {
                return Err(VmError::UndefVarError(name));
            }
        }
        Ok(())
    }

    /// Define or redefine a method from a runtime-`eval`'d function
    /// definition (Issue #8647): `eval(:(f(x) = 100))` and its `where`/
    /// multi-arg/varargs variants.
    ///
    /// SubsetJuliaVM is no-JIT/has-no-runtime-Expr-compiler by design
    /// (`AGENTS.md` Design Principle 1/7), so a method defined this way is
    /// never compiled to bytecode. Instead a tiny fixed trampoline body
    /// (`CallBuiltin(EvalDefinedCall, 0); ReturnAny`) is installed, and the
    /// stored `body` Expr is re-interpreted by the existing tree-walking
    /// `eval` machinery on every call — exactly how `@generated` staged
    /// bodies already work (see `eval_generated_expr_value`), just without a
    /// compiled generator step in front.
    ///
    /// A call site compiled *before* this runs (the common case: `f` already
    /// existed as a normal method) can only observe the redefinition if it
    /// still resolves to the *same* `functions` index — unlike the `@eval`
    /// macro form (`Stmt::EvalFunctionDef`), which is visible to the
    /// compiler and can add a brand-new index that name-based dynamic
    /// dispatch discovers. So when an existing same-name/-arity/no-vararg
    /// method with an all-`Any` signature is found (itself either a plain
    /// untyped-generic method or a prior eval-defined one), this OVERWRITES
    /// that `FunctionInfo` in place; a typed/kwarg/vararg-bearing method of
    /// the same name+arity is left untouched (it is strictly more specific
    /// than the `Any`-typed replacement as far as any *existing* call site
    /// is concerned) and a brand-new index is appended instead —
    /// discoverable only by later dynamic (`eval`/`include_string`-driven)
    /// calls, matching sjulia's AOT whole-program compile model (a plain,
    /// statically compiled call site can never observe a name that did not
    /// exist anywhere at compile time).
    ///
    /// Either way, `activate_eval_function` bumps `min_world` and calls
    /// `note_method_table_mutation`, so the Issue #8561 call-site inline
    /// caches are flushed exactly as they are for `@eval`.
    fn eval_define_function_from_expr(
        &mut self,
        signature_expr: &Value,
        body: Value,
        module_name: Option<&str>,
    ) -> Result<Value, VmError> {
        // Upstream evaluates a method's signature annotations EAGERLY, when the
        // definition executes — so a name that is not bound yet raises
        // `UndefVarError` at the definition, not at the first call
        // (`eval(:(f(x::NotYetDefined) = 1))`, verified against julia 1.12.6).
        // The compiled path already mirrors this with `Instr::LoadAny` probes
        // (`compile/stmt.rs::emit_signature_definition_probes`, Issues
        // #10396/#11025); the runtime-`eval` path skipped it entirely and fell
        // straight into `parse_call_signature`'s blanket
        // `VmError::NotImplemented` for typed parameters — which, under Issue
        // #8664's mapping, bound a raw `String` in the user's `catch` instead of
        // any `Exception` at all (Issue #11146; `types/signature_forward_reference_11025.jl`).
        self.probe_eval_signature_annotations(signature_expr, module_name)?;
        let sig = parse_call_signature(signature_expr)?;
        let arity = sig.param_names.len();

        let reuse_idx = if sig.vararg_index.is_none() {
            self.find_reusable_eval_target(&sig.name, arity)
        } else {
            None
        };

        // Install a fixed 2-instruction trampoline body. Mirrors
        // `install_specialized_body`'s runtime `self.code` append
        // (`vm/exec/call.rs`, Issue #8192 precedent): `Rc::make_mut`
        // copy-on-writes the shared bytecode vector, and `self.executable`
        // (the predecoded hot-loop-block index) is refreshed over the
        // appended range so its `block_by_ip` table stays in bounds.
        let entry = self.code.len();
        let code = std::rc::Rc::make_mut(&mut self.code);
        code.push(Instr::CallBuiltin(BuiltinId::EvalDefinedCall, 0));
        code.push(Instr::ReturnAny);
        let code_end = code.len();
        self.executable.append_bytecode(
            code,
            &self.functions,
            self.base_function_count,
            entry,
            code_end,
        );

        let params: Vec<(String, ValueType)> = sig
            .param_names
            .iter()
            .cloned()
            .map(|name| (name, ValueType::Any))
            .collect();
        let param_slots: Vec<usize> = (0..params.len()).collect();
        let slot_names = sig.param_names.clone();
        let local_slot_count = params.len();
        let param_julia_types = vec![JuliaType::Any; params.len()];

        let func_info = FunctionInfo {
            name: sig.name.clone(),
            params,
            kwparams: Vec::new(),
            entry,
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            is_generated: false,
            is_lowering_helper: false,
            definition_order: 0,
            // `activate_eval_function` sets the real `min_world` below; a
            // freshly installed method must not be visible before that.
            min_world: u64::MAX,
            type_params: Vec::new(),
            param_julia_types,
            code_start: entry,
            code_end,
            slot_names,
            slot_types: vec![None; local_slot_count],
            local_slot_count,
            param_slots,
            vararg_param_index: sig.vararg_index,
            vararg_fixed_count: None,
            inlining_meta: 0,
            constprop_meta: 0,
            nospecialize_meta: 0,
            propagate_inbounds_meta: false,
            nospecializeinfer_meta: false,
            purity_meta: 0,
            direct_return_type_param: None,
            def_line: 0,
            suppress_short_name_alias: false,
            shared_plan: None,
        };
        let slot_map: HashMap<String, usize> = func_info
            .slot_names
            .iter()
            .enumerate()
            .map(|(idx, name)| (name.clone(), idx))
            .collect();

        let idx = if let Some(idx) = reuse_idx {
            self.functions[idx] = std::rc::Rc::new(func_info);
            idx
        } else {
            let idx = self.functions.len();
            self.functions.push(std::rc::Rc::new(func_info));
            self.function_name_index
                .entry(sig.name.clone())
                .or_default()
                .push(idx);
            idx
        };
        while self.function_slot_maps.len() <= idx {
            self.function_slot_maps.push(HashMap::new());
        }
        self.function_slot_maps[idx] = slot_map;

        self.eval_defined_bodies.insert(
            idx,
            EvalDefinedMethod {
                body,
                module_name: module_name.map(str::to_string),
            },
        );

        // Bumps `min_world`, rebinds the global name to a generic
        // `Value::Function`, and flushes every dispatch/inline cache
        // (Issue #8561) — the exact same activation `@eval` uses.
        self.activate_eval_function(idx);

        Ok(Value::Function(FunctionValue::new(sig.name)))
    }

    /// Find an existing method to overwrite in place for a runtime-`eval`
    /// redefinition (Issue #8647): same name, same fixed arity, no vararg —
    /// and, critically, an index this same runtime-`eval` mechanism created
    /// (`self.eval_defined_bodies.contains_key`), never a method the
    /// compiler produced.
    ///
    /// A compile-time-produced `FunctionInfo` can be the `fallback_index` of
    /// a `SpecializableFunction` (Lazy AoT) whose `ir` is a *compile-time*
    /// snapshot never touched by this code, or be referenced by
    /// `i64_function_cache`/`f64_function_cache`/`typed_function_cache`/
    /// `specialization_cache`/`binary_method_cache`
    /// entries keyed by its index. Overwriting such an index in place would
    /// leave those caches silently pointing at stale/mismatched state —
    /// worse than doing nothing, since a later call could resolve through
    /// one of them and observe neither the old nor the new body. Restricting
    /// reuse to indices this function itself created sidesteps all of that:
    /// nothing outside `eval_defined_bodies` ever points at them.
    fn find_reusable_eval_target(&self, name: &str, arity: usize) -> Option<usize> {
        self.get_function_indices_by_name(name)
            .iter()
            .copied()
            .find(|&idx| {
                self.eval_defined_bodies.contains_key(&idx)
                    && self.functions.get(idx).is_some_and(|func| {
                        func.params.len() == arity && func.vararg_param_index.is_none()
                    })
            })
    }
}

/// Whether `value` is a call-shaped (optionally `where`-wrapped) assignment
/// target — the raw-AST signature of a short-form method definition like
/// `f(x) = expr` (Issue #8647). Detected structurally so this matches
/// exactly the same shape upstream `Meta.lower` treats as a method
/// definition, without special-casing on source text.
fn is_function_def_target(value: &Value) -> bool {
    let Value::Expr(expr) = value else {
        return false;
    };
    match ExprHead::from_expr(expr) {
        Some(ExprHead::Call) => true,
        Some(ExprHead::Where) => expr
            .args_snapshot()
            .first()
            .is_some_and(is_function_def_target),
        _ => false,
    }
}

/// Whether a registered type name (`registered`) denotes the annotation name
/// `wanted`, tolerating the two ways sjulia decorates a registration
/// (Issue #11146):
///
/// - parametric bases — `Box{Int64}` is registered for the base name `Box`;
/// - MODULE QUALIFICATION — a type declared inside `module M` is registered as
///   `M.Local`, and the VM tracks no "current module" during a plain `eval`, so
///   a definition eval'd from inside M sees only the bare `Local`.
///
/// The bare-suffix match is deliberately permissive: this predicate decides only
/// WHICH error an (always-erroring) typed-parameter `eval` definition raises, so
/// a false "bound" merely keeps the existing "typed parameters are deferred"
/// gap, while a false "unbound" would report a WRONG class (`UndefVarError` for
/// a type that plainly exists) — the very taxonomy sloppiness this issue
/// removes. An adversarial `codex exec` review of this diff caught exactly that
/// false positive for `module M; struct Local ... eval(:(f(x::Local) = 1))`.
fn type_registration_matches(registered: &str, wanted: &str) -> bool {
    let base = registered.split('{').next().unwrap_or(registered);
    base == wanted || base.rsplit('.').next().is_some_and(|last| last == wanted)
}

/// Collect the type names a method signature *evaluates* at definition time:
/// every `where`-binder's bound and every parameter annotation, minus the
/// binders themselves (`f(x::T) where {T<:Number}` evaluates `Number`, not `T`).
///
/// Mirrors the compiled path's probe set
/// (`compile/stmt.rs::{undeclared_where_bound_names,
/// append_undeclared_param_annotation_names}`, Issues #10396/#11025) for the
/// runtime-`eval` path (Issue #11146). Nested/compound annotations
/// (`Vector{Int}`, `Union{A,B}`, `Base.Number`) are left to their existing
/// permissive path, exactly as the compiled probes do.
fn eval_signature_annotation_names(signature: &Value) -> Vec<String> {
    fn bare_name(value: &Value) -> Option<String> {
        match value {
            Value::Symbol(s) => Some(s.as_str().to_string()),
            _ => None,
        }
    }

    let mut binders: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut call = signature.clone();

    // `where` first: upstream constructs the method's TypeVars before it
    // evaluates the parameter annotations, so an undefined BOUND raises first.
    if let Value::Expr(expr) = &signature {
        if ExprHead::is_expr(expr, ExprHead::Where) {
            let args = expr.args_snapshot();
            let mut iter = args.into_iter();
            if let Some(inner) = iter.next() {
                call = inner;
            }
            for param in iter {
                match &param {
                    // `where T` — a binder with no bound; nothing to evaluate.
                    Value::Symbol(s) => binders.push(s.as_str().to_string()),
                    // `where {T<:Bound}` — the binder is bound, the BOUND is evaluated.
                    Value::Expr(e) if ExprHead::is_expr(e, ExprHead::Subtype) => {
                        let sub_args = e.args_snapshot();
                        if let Some(name) = sub_args.first().and_then(bare_name) {
                            binders.push(name);
                        }
                        if let Some(bound) = sub_args.get(1).and_then(bare_name) {
                            names.push(bound);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if let Value::Expr(call_expr) = &call {
        if ExprHead::is_expr(call_expr, ExprHead::Call) {
            for param in call_expr.args_snapshot().iter().skip(1) {
                // `x::T` — the annotation is evaluated at definition time.
                let Value::Expr(e) = param else { continue };
                if !ExprHead::is_expr(e, ExprHead::TypeAssert) {
                    continue;
                }
                // `::T` (unnamed parameter) carries the annotation at index 0;
                // `x::T` carries it at index 1.
                let e_args = e.args_snapshot();
                let annotation = match e_args.len() {
                    1 => e_args.first(),
                    _ => e_args.get(1),
                };
                if let Some(name) = annotation.and_then(bare_name) {
                    names.push(name);
                }
            }
        }
    }

    names.retain(|n| !binders.contains(n));
    names.dedup();
    names
}

/// Signature extracted from a runtime-`eval`-defined function definition
/// (Issue #8647).
struct EvalFunctionSignature {
    name: String,
    /// Parameter names in declaration order. Every eval-defined parameter is
    /// treated as `::Any` — see `parse_call_signature` for what is deferred.
    param_names: Vec<String>,
    /// Index into `param_names` of a trailing `args...` vararg parameter.
    vararg_index: Option<usize>,
}

/// Extract `(name, params)` from a call-shaped (optionally `where`-wrapped)
/// method-definition target, e.g. `f(x, y)` or `f(x) where T` (Issue #8647).
///
/// Scope (documented, not silently dropped — deferred pending a follow-up):
/// only bare-Symbol parameter names and a single trailing `args...` are
/// accepted; typed parameters (`x::Int`), keyword parameters, and qualified
/// or parametric method names raise a clear `VmError::NotImplemented`
/// instead of silently mis-dispatching. `where`-bound type parameters are
/// accepted syntactically (so `f(x) where T` parses) but not enforced as a
/// bound, since no accepted parameter shape can reference them.
fn parse_call_signature(value: &Value) -> Result<EvalFunctionSignature, VmError> {
    let call_value = match value {
        Value::Expr(expr) if ExprHead::is_expr(expr, ExprHead::Where) => {
            match expr.args_snapshot().into_iter().next() {
                Some(inner) => inner,
                None => {
                    return Err(VmError::NotImplemented(
                        "eval: malformed `where` method signature (Issue #8647)".to_string(),
                    ))
                }
            }
        }
        other => other.clone(),
    };
    let Value::Expr(call_expr) = &call_value else {
        return Err(VmError::NotImplemented(format!(
            "eval: function definition signature must be a call expression, got {:?} (Issue #8647)",
            call_value.value_type()
        )));
    };
    if !ExprHead::is_expr(call_expr, ExprHead::Call) {
        return Err(VmError::NotImplemented(format!(
            "eval: function definition signature must be a call expression, got head '{}' (Issue #8647)",
            call_expr.head.as_str()
        )));
    }
    let args = call_expr.args_snapshot();
    let (callee, params) = args.split_first().ok_or_else(|| {
        VmError::NotImplemented(
            "eval: function definition requires a name (Issue #8647)".to_string(),
        )
    })?;
    let name = match callee {
        Value::Symbol(s) => s.as_str().to_string(),
        _ => {
            return Err(VmError::NotImplemented(format!(
                "eval: only a plain Symbol method name is supported for a runtime-eval'd \
                 function definition, got {:?} — qualified (Mod.f) and parametric (f{{T}}) \
                 names are deferred (Issue #8647)",
                callee.value_type()
            )))
        }
    };

    let mut param_names = Vec::with_capacity(params.len());
    let mut vararg_index = None;
    for (idx, param) in params.iter().enumerate() {
        match param {
            Value::Symbol(s) => param_names.push(s.as_str().to_string()),
            Value::Expr(e) if ExprHead::is_expr(e, ExprHead::Splat) => {
                match e.args_snapshot().first() {
                    Some(Value::Symbol(s)) => {
                        param_names.push(s.as_str().to_string());
                        vararg_index = Some(idx);
                    }
                    _ => {
                        return Err(VmError::NotImplemented(
                            "eval: unsupported varargs parameter shape in a runtime-eval'd \
                         function definition (Issue #8647)"
                                .to_string(),
                        ))
                    }
                }
            }
            _ => {
                return Err(VmError::NotImplemented(format!(
                    "eval: typed and keyword parameters in a runtime-eval'd function \
                     definition are deferred, got {:?} (Issue #8647)",
                    param.value_type()
                )))
            }
        }
    }

    Ok(EvalFunctionSignature {
        name,
        param_names,
        vararg_index,
    })
}

#[cfg(test)]
mod tests;
