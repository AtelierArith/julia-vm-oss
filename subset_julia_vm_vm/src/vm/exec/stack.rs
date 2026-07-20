//! Stack operations for the VM.
//!
//! This module handles stack instructions including:
//! - Push* instructions for constant values
//! - Pop: discard top of stack
//! - Swap: swap top two values
//! - MakeRef/UnwrapRef: reference wrapping for broadcast protection

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;
use crate::types::JuliaType;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{
    ClosureValue, ExprValue, FunctionValue, LineNumberNodeValue, ModuleValue, SymbolValue,
    TupleValue, Value,
};
use super::super::Vm;
use super::DispatchAction;

impl<R: RngLike> Vm<R> {
    #[inline(always)]
    fn push_continue(&mut self, value: Value) -> Result<DispatchAction, VmError> {
        self.stack.push(value);
        Ok(DispatchAction::Continue)
    }

    fn capture_closure_environment(
        &self,
        func_name: &str,
        capture_names: &[String],
    ) -> Result<Vec<(String, Value)>, VmError> {
        let mut captures = Vec::with_capacity(capture_names.len());
        let frame_idx = self.frames.len().saturating_sub(1);

        for name in capture_names {
            // A closure created directly by module/main lexical code captures
            // that environment before consulting frame 0. Called functions
            // deliberately skip it so their global lookup cannot inherit a
            // caller's lexical shadow (Issues #11569/#9784).
            if frame_idx == 0 {
                if let Some(binding) = self.root_lexical_binding(name) {
                    if let Some(value) = binding {
                        captures.push((name.clone(), value.clone()));
                        continue;
                    }
                    return Err(VmError::UndefVarError(format!(
                        "Cannot capture undefined variable: {}",
                        name
                    )));
                }
            }
            if let Some(value) = self.get_value_from_frame(name, frame_idx) {
                captures.push((name.clone(), value));
            } else if frame_idx != 0
                && self
                    .get_value_from_frame(name, 0)
                    .map(|value| captures.push((name.clone(), value)))
                    .is_some()
            {
                // Closures nested at depth >= 2 have no module-global slot in
                // their own frame. Snapshot that binding from frame 0, matching
                // the single-level path (Issue #7600).
            } else if let Some(sibling) = self.resolve_sibling_nested_function(func_name, name) {
                // Mutually-recursive nested functions can capture a sibling or
                // self that has no live enclosing-frame slot (Issue #8118).
                captures.push((name.clone(), sibling));
            } else {
                return Err(VmError::UndefVarError(format!(
                    "Cannot capture undefined variable: {}",
                    name
                )));
            }
        }

        Ok(captures)
    }

    /// Execute stack push instructions for constant values.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    // Hot dispatch handler: front-loaded in `dispatch_instr` (Issue #5175).
    #[inline(always)]
    pub(super) fn execute_stack(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::PushI64(x) => self.push_continue(Value::I64(*x)),
            Instr::PushI128(x) => self.push_continue(Value::I128(**x)),
            Instr::PushBigInt(s) => {
                self.push_continue(Value::BigInt(s.parse().unwrap_or_default()))
            }
            Instr::PushBigFloat(s) => {
                self.push_continue(Value::BigFloat(s.parse().unwrap_or_default()))
            }
            Instr::PushF64(x) => self.push_continue(Value::F64(*x)),
            Instr::PushF32(x) => self.push_continue(Value::F32(*x)),
            Instr::PushF16(x) => self.push_continue(Value::F16(*x)),
            Instr::PushBool(b) => self.push_continue(Value::Bool(*b)),
            Instr::PushBoundsCheckEnabled => {
                let enabled = self
                    .frames
                    .last()
                    .map(|frame| !frame.inbounds_context)
                    .unwrap_or(true);
                self.push_continue(Value::Bool(enabled))
            }
            Instr::PushStr(s) => self.push_continue(Value::str_new(s.clone())),
            Instr::PushStrBytes(bytes) => self.push_continue(Value::str_from_bytes(bytes.clone())),
            Instr::PushChar(c) => self.push_continue(Value::Char(*c)),
            Instr::PushCharMalformed(bits) => self.push_continue(Value::CharMalformed(*bits)),
            Instr::PushNothing => self.push_continue(Value::Nothing),
            Instr::PushMissing => self.push_continue(Value::Missing),
            Instr::PushUndef => self.push_continue(Value::Undef),
            Instr::PushStdout => self.push_continue(Value::IO(self.current_stdout.clone())),
            Instr::PushStderr => self.push_continue(Value::IO(self.current_stderr.clone())),
            Instr::PushStdin => self.push_continue(Value::IO(self.stdin_stream.clone())),
            Instr::PushDevnull => self.push_continue(Value::IO(self.devnull_stream.clone())),
            Instr::PushCNull => {
                // C_NULL is Ptr{Cvoid}(0) - a null pointer
                // We represent it as I64(0) since we don't have full pointer support
                self.push_continue(Value::I64(0))
            }
            Instr::PushEnv => {
                // ENV is a pure-Julia `Dict{String,String}` of environment
                // variables. PushEnv only supplies the raw OS pairs as a tuple of
                // `(key, value)` 2-tuples; the compiler routes the result through
                // the pure `_env_from_pairs` helper, which builds the struct via
                // the ordinary `Dict{String,String}(kv)` constructor (Issue #6731).
                let pairs: Vec<Value> = std::env::vars()
                    .map(|(key, value)| {
                        Value::Tuple(TupleValue::new(vec![
                            Value::str_new(key),
                            Value::str_new(value),
                        ]))
                    })
                    .collect();
                self.push_continue(Value::Tuple(TupleValue::new(pairs)))
            }
            Instr::PushModule(operands) => {
                self.push_continue(Value::Module(Box::new(ModuleValue::with_exports_publics(
                    operands.name.clone(),
                    operands.exports.clone(),
                    operands.publics.clone(),
                    operands.base_exports_visible,
                    operands.implicit_standard_bindings,
                ))))
            }
            Instr::PushDataType(type_name) => {
                if self.eval_nominal_type_name_is_unpublished(type_name) {
                    let local_name = type_name
                        .rsplit('.')
                        .next()
                        .unwrap_or(type_name)
                        .to_string();
                    self.raise(VmError::UndefVarError(local_name))?;
                    return Ok(DispatchAction::Continue);
                }
                // An `@enum` type name resolves to `JuliaType::Enum`, so that the
                // bare type value (`Color`) is `===` to `typeof(red)` which also
                // projects to `JuliaType::Enum("Color")` (Issue #5139). The
                // registry is populated by `RegisterEnum`, which runs before any
                // reference to the type can be reached.
                let julia_type = if crate::vm::value::enum_registry::is_registered_enum(type_name) {
                    JuliaType::Enum(type_name.clone())
                } else {
                    self.datatype_from_name_or_partial_unionall(type_name)
                };
                self.push_continue(Value::DataType(Box::new(julia_type)))
            }
            Instr::PushFunction(name) => {
                self.push_continue(Value::Function(FunctionValue::new(name.clone())))
            }
            Instr::PushResolvedFunction(operands) => {
                let function = self.function_value_with_candidates(
                    operands.name.clone(),
                    operands.candidate_indices.clone(),
                );
                self.push_continue(Value::Function(function))
            }
            Instr::CreateClosure {
                func_name,
                capture_names,
            } => {
                let captures = self.capture_closure_environment(func_name, capture_names)?;
                self.push_continue(Value::Closure(ClosureValue::new(
                    func_name.clone(),
                    captures,
                )))
            }
            Instr::CreateResolvedClosure(operands) => {
                let captures =
                    self.capture_closure_environment(&operands.name, &operands.capture_names)?;
                let closure = self.closure_value_with_candidates(
                    operands.name.clone(),
                    captures,
                    operands.candidate_indices.clone(),
                );
                self.push_continue(Value::Closure(closure))
            }
            Instr::LoadCaptured(name) => {
                // Load a captured variable from the current frame's closure environment
                let frame = self.frames.last().ok_or_else(|| {
                    VmError::InternalError("No frame for captured variable lookup".to_string())
                })?;

                if let Some(value) = frame.captured_vars.get(name) {
                    self.stack.push(value.clone());
                    Ok(DispatchAction::Continue)
                } else {
                    Err(VmError::UndefVarError(format!(
                        "Captured variable not found: {}",
                        name
                    )))
                }
            }
            Instr::DefineFunction(func_idx) => Ok(self.execute_define_function(*func_idx)),
            Instr::DefineEvalFunction(func_idx) => Ok(self.execute_define_eval_function(*func_idx)),
            Instr::ActivateUsing {
                owner_module,
                program_index,
            } => Ok(self.execute_activate_using(owner_module, *program_index)),
            Instr::ActivateModule(module_path) => {
                self.record_repl_module_activation(module_path);
                Ok(DispatchAction::Continue)
            }
            Instr::DefineEvalStruct(type_id) => {
                if let Err(error) = self.activate_eval_struct(*type_id) {
                    self.raise(error)?;
                }
                Ok(DispatchAction::Continue)
            }
            Instr::DefineEvalAbstractType(type_id) => {
                if let Err(error) = self.activate_eval_abstract_type(*type_id) {
                    self.raise(error)?;
                }
                Ok(DispatchAction::Continue)
            }
            Instr::DefineEvalPrimitiveType(type_id) => {
                if let Err(error) = self.activate_eval_primitive_type(*type_id) {
                    self.raise(error)?;
                }
                Ok(DispatchAction::Continue)
            }
            Instr::DefineRuntimeNominal(operands) => {
                if let Err(error) = self.define_runtime_nominal(operands) {
                    self.raise(error)?;
                }
                Ok(DispatchAction::Continue)
            }

            // Stack manipulation
            Instr::Pop => {
                // Just discard the top of the stack
                self.stack.pop();
                Ok(DispatchAction::Continue)
            }
            Instr::PopIfIO => {
                // Pop if IO type, otherwise leave on stack (for runtime IO detection in print)
                if let Some(val) = self.stack.last() {
                    if matches!(val, Value::IO(_)) {
                        self.stack.pop();
                    }
                }
                Ok(DispatchAction::Continue)
            }
            Instr::Swap => {
                // Swap top two values on stack
                let len = self.stack.len();
                if len >= 2 {
                    self.stack.swap(len - 1, len - 2);
                }
                Ok(DispatchAction::Continue)
            }

            // Ref operations: build/unwrap Base.RefValue{T} (Issue #5130);
            // also serves as the broadcast scalar wrapper.
            Instr::MakeRef => {
                let val = self.stack.pop_value()?;
                self.stack.push(crate::vm::value::new_ref(val));
                Ok(DispatchAction::Continue)
            }
            Instr::UnwrapRef => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::Ref(inner) => {
                        let v = inner.borrow().clone();
                        self.stack.push(v);
                    }
                    other => self.stack.push(other), // Non-Ref values pass through
                }
                Ok(DispatchAction::Continue)
            }

            // Metaprogramming value instructions (for REPL persistence)
            Instr::PushSymbol(name) => self.push_continue(Value::Symbol(SymbolValue::new(name))),
            Instr::CreateExpr { head, arg_count } => {
                // Pop arg_count values from stack (in reverse order)
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse(); // Restore original order
                let expr = ExprValue::new(SymbolValue::new(head), args);
                self.push_continue(Value::Expr(expr))
            }
            Instr::CreateQuoteNode => {
                let val = self.stack.pop_value()?;
                self.push_continue(Value::QuoteNode(Box::new(val)))
            }
            Instr::PushLineNumberNode { line, file } => self.push_continue(Value::LineNumberNode(
                LineNumberNodeValue::new(*line, file.clone()),
            )),
            Instr::PushRegex { pattern, flags } => {
                use crate::vm::value::RegexValue;
                match RegexValue::new(pattern, flags) {
                    Ok(regex) => self.push_continue(Value::Regex(Box::new(regex))),
                    Err(e) => Err(VmError::TypeError(format!("Invalid regex: {}", e))),
                }
            }
            Instr::PushEnum { type_name, value } => {
                if self.eval_enum_type_name_is_unpublished(type_name)
                    || self.runtime_nominal_enum_type_name_is_unpublished(type_name)
                {
                    let local_name = type_name
                        .rsplit('.')
                        .next()
                        .unwrap_or(type_name)
                        .to_string();
                    self.raise(VmError::UndefVarError(local_name))?;
                    return Ok(DispatchAction::Continue);
                }
                if let Err(error) = self.validate_eval_enum_member_push(type_name, *value) {
                    self.raise(error)?;
                    return Ok(DispatchAction::Continue);
                }
                self.push_continue(Value::Enum {
                    type_name: type_name.clone(),
                    value: *value,
                })
            }
            Instr::RegisterEnum(operands) => {
                // Publish the source-ordered REPL enum definition, then populate
                // the formatting registry. Ordinary non-REPL programs have no
                // pending tail and preserve the historical direct registration.
                if let Err(error) = self.activate_eval_enum(operands) {
                    self.raise(error)?;
                }
                Ok(DispatchAction::Continue)
            }
            Instr::ConstructEnum(type_name) => {
                if self.runtime_nominal_enum_type_name_is_unpublished(type_name) {
                    let local_name = type_name
                        .rsplit('.')
                        .next()
                        .unwrap_or(type_name)
                        .to_string();
                    self.raise(VmError::UndefVarError(local_name))?;
                    return Ok(DispatchAction::Continue);
                }
                // `Color(value)`: validate the popped integer against the
                // registered members and push the corresponding enum value.
                let val = self.stack.pop_value()?;
                let value = self.convert_to_i64(&val)?;
                if crate::vm::value::enum_registry::is_valid_value(type_name, value) {
                    self.push_continue(Value::Enum {
                        type_name: type_name.clone(),
                        value,
                    })
                } else {
                    // Matches upstream `ArgumentError("invalid value for Enum
                    // Color: 5")`. Raised through the real ArgumentError variant
                    // since Issue #11146 — it previously used the TypeError
                    // variant with an ArgumentError text prefix, so
                    // `typeof(caught)` contradicted the message.
                    Err(VmError::ArgumentError(format!(
                        "invalid value for Enum {}: {}",
                        type_name, value
                    )))
                }
            }

            _ => Err(super::unhandled(instr)),
        }
    }

    fn execute_define_function(&mut self, func_idx: usize) -> DispatchAction {
        // Runtime block definitions already have a compiled function-table row.
        self.record_repl_runtime_function(func_idx);
        DispatchAction::Continue
    }

    fn execute_define_eval_function(&mut self, func_idx: usize) -> DispatchAction {
        self.activate_eval_function(func_idx);
        DispatchAction::Continue
    }

    fn execute_activate_using(
        &mut self,
        owner_module: &str,
        program_index: usize,
    ) -> DispatchAction {
        self.record_repl_using_activation(owner_module, program_index);
        DispatchAction::Continue
    }

    /// Resolve a closure capture `name` that has no live binding in the enclosing
    /// frame to a sibling / self nested function value, by qualifying it against
    /// each enclosing lexical scope of the closure's own qualified `func_name`
    /// (`a#b#c` → try `a#b#name`, then `a#name`) and returning a by-name function
    /// value for the first qualified name that names a known function. Returns
    /// `None` when no such nested function exists. Supports mutually-recursive
    /// nested functions, where one closure captures a (possibly forward) sibling
    /// that is not yet bound as a local when the closure is built (Issue #8118).
    fn resolve_sibling_nested_function(&self, func_name: &str, name: &str) -> Option<Value> {
        let segments: Vec<&str> = func_name.split('#').collect();
        // Enclosing scopes only: drop the closure's own trailing segment.
        for depth in (1..segments.len()).rev() {
            let candidate = format!("{}#{}", segments[..depth].join("#"), name);
            if !self.get_function_indices_by_name(&candidate).is_empty() {
                return Some(Value::Function(FunctionValue::new(candidate)));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::StableRng;

    #[test]
    fn push_continue_pushes_value_and_returns_continue_issue_10260() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));

        let action = match vm.push_continue(Value::I64(42)) {
            Ok(action) => action,
            Err(err) => panic!("push_continue should not fail: {err}"),
        };

        assert!(matches!(action, DispatchAction::Continue));
        assert!(matches!(vm.stack.pop(), Some(Value::I64(42))));
    }
}
