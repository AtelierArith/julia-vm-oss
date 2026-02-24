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
    ClosureValue, DictKey, DictValue, ExprValue, FunctionValue, IOValue, LineNumberNodeValue,
    ModuleValue, SymbolValue, Value,
};
use super::super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute stack push instructions for constant values.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    #[inline]
    pub(super) fn execute_stack(&mut self, instr: &Instr) -> Result<Option<()>, VmError> {
        match instr {
            Instr::PushI64(x) => {
                self.stack.push(Value::I64(*x));
                Ok(Some(()))
            }
            Instr::PushI128(x) => {
                self.stack.push(Value::I128(*x));
                Ok(Some(()))
            }
            Instr::PushBigInt(s) => {
                self.stack
                    .push(Value::BigInt(s.parse().unwrap_or_default()));
                Ok(Some(()))
            }
            Instr::PushBigFloat(s) => {
                self.stack
                    .push(Value::BigFloat(s.parse().unwrap_or_default()));
                Ok(Some(()))
            }
            Instr::PushF64(x) => {
                self.stack.push(Value::F64(*x));
                Ok(Some(()))
            }
            Instr::PushF32(x) => {
                self.stack.push(Value::F32(*x));
                Ok(Some(()))
            }
            Instr::PushF16(x) => {
                self.stack.push(Value::F16(*x));
                Ok(Some(()))
            }
            Instr::PushBool(b) => {
                self.stack.push(Value::Bool(*b));
                Ok(Some(()))
            }
            Instr::PushStr(s) => {
                self.stack.push(Value::Str(s.clone()));
                Ok(Some(()))
            }
            Instr::PushChar(c) => {
                self.stack.push(Value::Char(*c));
                Ok(Some(()))
            }
            Instr::PushNothing => {
                self.stack.push(Value::Nothing);
                Ok(Some(()))
            }
            Instr::PushMissing => {
                self.stack.push(Value::Missing);
                Ok(Some(()))
            }
            Instr::PushUndef => {
                self.stack.push(Value::Undef);
                Ok(Some(()))
            }
            Instr::PushStdout => {
                self.stack.push(Value::IO(IOValue::stdout_ref()));
                Ok(Some(()))
            }
            Instr::PushStderr => {
                self.stack.push(Value::IO(IOValue::stderr_ref()));
                Ok(Some(()))
            }
            Instr::PushStdin => {
                self.stack.push(Value::IO(IOValue::stdin_ref()));
                Ok(Some(()))
            }
            Instr::PushDevnull => {
                self.stack.push(Value::IO(IOValue::devnull_ref()));
                Ok(Some(()))
            }
            Instr::PushCNull => {
                // C_NULL is Ptr{Cvoid}(0) - a null pointer
                // We represent it as I64(0) since we don't have full pointer support
                self.stack.push(Value::I64(0));
                Ok(Some(()))
            }
            Instr::PushEnv => {
                // ENV is a Dict{String,String} containing environment variables
                // Read from the actual OS environment at runtime
                let mut entries = Vec::new();
                for (key, value) in std::env::vars() {
                    entries.push((DictKey::Str(key), Value::Str(value)));
                }
                self.stack
                    .push(Value::Dict(Box::new(DictValue::with_entries(entries))));
                Ok(Some(()))
            }
            Instr::PushModule(name, exports, publics) => {
                self.stack
                    .push(Value::Module(Box::new(ModuleValue::with_exports_publics(
                        name.clone(),
                        exports.clone(),
                        publics.clone(),
                    ))));
                Ok(Some(()))
            }
            Instr::PushDataType(type_name) => {
                let julia_type = JuliaType::from_name_or_struct(type_name);
                self.stack.push(Value::DataType(julia_type));
                Ok(Some(()))
            }
            Instr::PushFunction(name) => {
                self.stack
                    .push(Value::Function(FunctionValue::new(name.clone())));
                Ok(Some(()))
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
                Ok(Some(()))
            }
            Instr::LoadCaptured(name) => {
                // Load a captured variable from the current frame's closure environment
                let frame = self.frames.last().ok_or_else(|| {
                    VmError::InternalError("No frame for captured variable lookup".to_string())
                })?;

                if let Some(value) = frame.captured_vars.get(name) {
                    self.stack.push(value.clone());
                    Ok(Some(()))
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
                Ok(Some(()))
            }

            // Stack manipulation
            Instr::Pop => {
                // Just discard the top of the stack
                self.stack.pop();
                Ok(Some(()))
            }
            Instr::PopIfIO => {
                // Pop if IO type, otherwise leave on stack (for runtime IO detection in print)
                if let Some(val) = self.stack.last() {
                    if matches!(val, Value::IO(_)) {
                        self.stack.pop();
                    }
                }
                Ok(Some(()))
            }
            Instr::Swap => {
                // Swap top two values on stack
                let len = self.stack.len();
                if len >= 2 {
                    self.stack.swap(len - 1, len - 2);
                }
                Ok(Some(()))
            }

            // Ref operations (broadcast protection)
            Instr::MakeRef => {
                let val = self.stack.pop_value()?;
                self.stack.push(Value::Ref(Box::new(val)));
                Ok(Some(()))
            }
            Instr::UnwrapRef => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::Ref(inner) => self.stack.push(*inner),
                    other => self.stack.push(other), // Non-Ref values pass through
                }
                Ok(Some(()))
            }

            // Metaprogramming value instructions (for REPL persistence)
            Instr::PushSymbol(name) => {
                self.stack.push(Value::Symbol(SymbolValue::new(name)));
                Ok(Some(()))
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
                Ok(Some(()))
            }
            Instr::CreateQuoteNode => {
                let val = self.stack.pop_value()?;
                self.stack.push(Value::QuoteNode(Box::new(val)));
                Ok(Some(()))
            }
            Instr::PushLineNumberNode { line, file } => {
                self.stack
                    .push(Value::LineNumberNode(LineNumberNodeValue::new(
                        *line,
                        file.clone(),
                    )));
                Ok(Some(()))
            }
            Instr::PushRegex { pattern, flags } => {
                use crate::vm::value::RegexValue;
                match RegexValue::new(pattern, flags) {
                    Ok(regex) => {
                        self.stack.push(Value::Regex(regex));
                        Ok(Some(()))
                    }
                    Err(e) => Err(VmError::TypeError(format!("Invalid regex: {}", e))),
                }
            }
            Instr::PushEnum { type_name, value } => {
                self.stack.push(Value::Enum {
                    type_name: type_name.clone(),
                    value: *value,
                });
                Ok(Some(()))
            }

            _ => Ok(None),
        }
    }
}
