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
    ClosureValue, ExprValue, FunctionValue, IOValue, LineNumberNodeValue, ModuleValue, SymbolValue,
    TupleValue, Value,
};
use super::super::Vm;
use super::DispatchAction;

impl<R: RngLike> Vm<R> {
    /// Execute stack push instructions for constant values.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    // Hot dispatch handler: front-loaded in `dispatch_instr` (Issue #5175).
    #[inline(always)]
    pub(super) fn execute_stack(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::PushI64(x) => {
                self.stack.push(Value::I64(*x));
                Ok(DispatchAction::Continue)
            }
            Instr::PushI128(x) => {
                self.stack.push(Value::I128(**x));
                Ok(DispatchAction::Continue)
            }
            Instr::PushBigInt(s) => {
                self.stack
                    .push(Value::BigInt(s.parse().unwrap_or_default()));
                Ok(DispatchAction::Continue)
            }
            Instr::PushBigFloat(s) => {
                self.stack
                    .push(Value::BigFloat(s.parse().unwrap_or_default()));
                Ok(DispatchAction::Continue)
            }
            Instr::PushF64(x) => {
                self.stack.push(Value::F64(*x));
                Ok(DispatchAction::Continue)
            }
            Instr::PushF32(x) => {
                self.stack.push(Value::F32(*x));
                Ok(DispatchAction::Continue)
            }
            Instr::PushF16(x) => {
                self.stack.push(Value::F16(*x));
                Ok(DispatchAction::Continue)
            }
            Instr::PushBool(b) => {
                self.stack.push(Value::Bool(*b));
                Ok(DispatchAction::Continue)
            }
            Instr::PushBoundsCheckEnabled => {
                let enabled = self
                    .frames
                    .last()
                    .map(|frame| !frame.inbounds_context)
                    .unwrap_or(true);
                self.stack.push(Value::Bool(enabled));
                Ok(DispatchAction::Continue)
            }
            Instr::PushStr(s) => {
                self.stack.push(Value::Str(s.clone()));
                Ok(DispatchAction::Continue)
            }
            Instr::PushChar(c) => {
                self.stack.push(Value::Char(*c));
                Ok(DispatchAction::Continue)
            }
            Instr::PushNothing => {
                self.stack.push(Value::Nothing);
                Ok(DispatchAction::Continue)
            }
            Instr::PushMissing => {
                self.stack.push(Value::Missing);
                Ok(DispatchAction::Continue)
            }
            Instr::PushUndef => {
                self.stack.push(Value::Undef);
                Ok(DispatchAction::Continue)
            }
            Instr::PushStdout => {
                self.stack.push(Value::IO(IOValue::stdout_ref()));
                Ok(DispatchAction::Continue)
            }
            Instr::PushStderr => {
                self.stack.push(Value::IO(IOValue::stderr_ref()));
                Ok(DispatchAction::Continue)
            }
            Instr::PushStdin => {
                self.stack.push(Value::IO(IOValue::stdin_ref()));
                Ok(DispatchAction::Continue)
            }
            Instr::PushDevnull => {
                self.stack.push(Value::IO(IOValue::devnull_ref()));
                Ok(DispatchAction::Continue)
            }
            Instr::PushCNull => {
                // C_NULL is Ptr{Cvoid}(0) - a null pointer
                // We represent it as I64(0) since we don't have full pointer support
                self.stack.push(Value::I64(0));
                Ok(DispatchAction::Continue)
            }
            Instr::PushEnv => {
                // ENV is a pure-Julia `Dict{String,String}` of environment
                // variables. PushEnv only supplies the raw OS pairs as a tuple of
                // `(key, value)` 2-tuples; the compiler routes the result through
                // the pure `_env_from_pairs` helper, which builds the struct via
                // the ordinary `Dict{String,String}(kv)` constructor (Issue #6731).
                let pairs: Vec<Value> = std::env::vars()
                    .map(|(key, value)| {
                        Value::Tuple(TupleValue::new(vec![Value::Str(key), Value::Str(value)]))
                    })
                    .collect();
                self.stack.push(Value::Tuple(TupleValue::new(pairs)));
                Ok(DispatchAction::Continue)
            }
            Instr::PushModule(operands) => {
                self.stack
                    .push(Value::Module(Box::new(ModuleValue::with_exports_publics(
                        operands.name.clone(),
                        operands.exports.clone(),
                        operands.publics.clone(),
                    ))));
                Ok(DispatchAction::Continue)
            }
            Instr::PushDataType(type_name) => {
                // An `@enum` type name resolves to `JuliaType::Enum`, so that the
                // bare type value (`Color`) is `===` to `typeof(red)` which also
                // projects to `JuliaType::Enum("Color")` (Issue #5139). The
                // registry is populated by `RegisterEnum`, which runs before any
                // reference to the type can be reached.
                let julia_type = if crate::vm::value::enum_registry::is_registered_enum(type_name) {
                    JuliaType::Enum(type_name.clone())
                } else {
                    JuliaType::from_name_or_struct(type_name)
                };
                self.stack.push(Value::DataType(Box::new(julia_type)));
                Ok(DispatchAction::Continue)
            }
            Instr::PushFunction(name) => {
                self.stack
                    .push(Value::Function(FunctionValue::new(name.clone())));
                Ok(DispatchAction::Continue)
            }
            Instr::PushResolvedFunction(operands) => {
                self.stack
                    .push(Value::Function(FunctionValue::with_candidates(
                        operands.name.clone(),
                        operands.candidate_indices.clone(),
                    )));
                Ok(DispatchAction::Continue)
            }
            Instr::CreateClosure {
                func_name,
                capture_names,
            } => {
                // Create a closure by capturing variables from the current frame
                let mut captures = Vec::with_capacity(capture_names.len());
                let frame_idx = self.frames.len().saturating_sub(1);

                for name in capture_names {
                    // Look up the variable in the current frame using get_value_from_frame
                    if let Some(value) = self.get_value_from_frame(name, frame_idx) {
                        captures.push((name.clone(), value));
                    } else if frame_idx != 0
                        && self
                            .get_value_from_frame(name, 0)
                            .map(|value| captures.push((name.clone(), value)))
                            .is_some()
                    {
                        // Global/const/builtin fallback for closures nested at
                        // depth >= 2 (Issue #7600). When `CreateClosure` runs at
                        // the top-level frame (single-level closures), an unknown
                        // capture name resolves against the global frame as part
                        // of `get_value_from_frame`. Once the closure is nested
                        // inside another closure/do-block, `CreateClosure` runs in
                        // the enclosing closure's frame, which holds no slot for a
                        // module global/const/builtin (`pi`, a user `const`, or a
                        // non-const global). Mirror the single-level behaviour by
                        // snapshotting the binding from the global frame (frame 0)
                        // instead of raising `Cannot capture undefined variable`.
                    } else if let Some(sibling) =
                        self.resolve_sibling_nested_function(func_name, name)
                    {
                        // Mutually-recursive nested functions (Issue #8118): a
                        // closure may capture a sibling / self nested function that
                        // has no live slot in the enclosing frame (e.g. a forward
                        // sibling defined after this closure). Resolve such a
                        // capture to a by-name function value for the enclosing
                        // scope's qualified `parent#name`, so the reconstructed
                        // closure can call it through its captured environment.
                        captures.push((name.clone(), sibling));
                    } else {
                        return Err(VmError::UndefVarError(format!(
                            "Cannot capture undefined variable: {}",
                            name
                        )));
                    }
                }

                self.stack.push(Value::Closure(ClosureValue::new(
                    func_name.clone(),
                    captures,
                )));
                Ok(DispatchAction::Continue)
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
            Instr::DefineFunction(func_idx) => {
                // Define a function at runtime (for functions defined inside blocks like @testset).
                // The function is already compiled and stored in function_infos at index func_idx.
                // We just need to mark it as "active" by adding it to the dispatch table.
                // The function is already in the function_infos table, so it can be called by name.
                // This instruction is a no-op at runtime since the function is already available.
                // It serves as a marker that the function definition was executed.
                let _ = func_idx; // Function is already compiled and indexed
                Ok(DispatchAction::Continue)
            }
            Instr::DefineEvalFunction(func_idx) => {
                self.activate_eval_function(*func_idx);
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
            Instr::PushSymbol(name) => {
                self.stack.push(Value::Symbol(SymbolValue::new(name)));
                Ok(DispatchAction::Continue)
            }
            Instr::CreateExpr { head, arg_count } => {
                // Pop arg_count values from stack (in reverse order)
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse(); // Restore original order
                let expr = ExprValue::new(SymbolValue::new(head), args);
                self.stack.push(Value::Expr(expr));
                Ok(DispatchAction::Continue)
            }
            Instr::CreateQuoteNode => {
                let val = self.stack.pop_value()?;
                self.stack.push(Value::QuoteNode(Box::new(val)));
                Ok(DispatchAction::Continue)
            }
            Instr::PushLineNumberNode { line, file } => {
                self.stack
                    .push(Value::LineNumberNode(LineNumberNodeValue::new(
                        *line,
                        file.clone(),
                    )));
                Ok(DispatchAction::Continue)
            }
            Instr::PushRegex { pattern, flags } => {
                use crate::vm::value::RegexValue;
                match RegexValue::new(pattern, flags) {
                    Ok(regex) => {
                        self.stack.push(Value::Regex(Box::new(regex)));
                        Ok(DispatchAction::Continue)
                    }
                    Err(e) => Err(VmError::TypeError(format!("Invalid regex: {}", e))),
                }
            }
            Instr::PushEnum { type_name, value } => {
                self.stack.push(Value::Enum {
                    type_name: type_name.clone(),
                    value: *value,
                });
                Ok(DispatchAction::Continue)
            }
            Instr::RegisterEnum(operands) => {
                // Populate the thread-local enum registry (Issue #5139) so
                // display / construction / `instances` can recover member names.
                crate::vm::value::enum_registry::register_enum(
                    &operands.type_name,
                    &operands.members,
                );
                Ok(DispatchAction::Continue)
            }
            Instr::ConstructEnum(type_name) => {
                // `Color(value)`: validate the popped integer against the
                // registered members and push the corresponding enum value.
                let val = self.stack.pop_value()?;
                let value = self.convert_to_i64(&val)?;
                if crate::vm::value::enum_registry::is_valid_value(type_name, value) {
                    self.stack.push(Value::Enum {
                        type_name: type_name.clone(),
                        value,
                    });
                    Ok(DispatchAction::Continue)
                } else {
                    // Matches upstream `ArgumentError("invalid value for Enum
                    // Color: 5")`; surfaced via `TypeError` with the
                    // `ArgumentError:` prefix, the VM's catchable convention.
                    Err(VmError::TypeError(format!(
                        "ArgumentError: invalid value for Enum {}: {}",
                        type_name, value
                    )))
                }
            }

            _ => Err(super::unhandled(instr)),
        }
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
