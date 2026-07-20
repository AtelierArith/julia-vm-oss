//! Error handling and testing operations for the VM.
//!
//! This module handles error and test instructions:
//! - ThrowError: Throw an error
//! - Test, TestSetBegin, TestSetEnd: Testing framework
//! - TestThrowsBegin, TestThrowsEnd: Test that code throws
//! - PushHandler, PopHandler: Exception handlers
//! - ClearError, PushErrorCode, PushErrorMessage: Error state
//! - Rethrow: Re-throw pending error

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::formatting::resolve_struct_refs_for_format;
use super::super::frame::Handler;
use super::super::instr::Instr;
use super::super::util::{format_value, format_value_print, Resolved};
use super::super::value::Value;
use super::super::Vm;
use super::exception_payload::PendingExceptionPayload;
use super::DispatchAction;

/// Reconstruct the `(func::Symbol, args)` fields of the Julia `InexactError`
/// struct from the `"{T}({val})"` message carried by [`VmError::InexactError`]
/// (Issue #8212). The message shape is produced uniformly by the conversion
/// paths (`vm/convert.rs`), e.g. `"Int64(1.5)"`. Unparsable values fall back to
/// a `Str`, and a message without the `(...)` shape falls back to placeholder
/// fields — in both cases the reconstructed struct still has the correct type so
/// `typeof(e) == InexactError` / `isa(e, InexactError)` hold.
fn inexact_error_fields(msg: &str) -> Vec<Value> {
    use crate::vm::value::{SymbolValue, TupleValue};
    match msg.find('(') {
        Some(open) => {
            let type_name = &msg[..open];
            let inner = msg
                .get(open + 1..)
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or("");
            let func = Value::Symbol(SymbolValue::new(type_name));
            let t = Value::DataType(Box::new(crate::types::JuliaType::from_name_or_struct(
                type_name,
            )));
            let val = if let Ok(i) = inner.parse::<i64>() {
                Value::I64(i)
            } else if let Ok(f) = inner.parse::<f64>() {
                Value::F64(f)
            } else {
                Value::str_new(inner.to_string())
            };
            vec![func, Value::Tuple(TupleValue::new(vec![t, val]))]
        }
        None => vec![
            Value::Symbol(SymbolValue::new("unknown")),
            Value::Tuple(TupleValue::new(vec![Value::str_new(msg.to_string())])),
        ],
    }
}

impl<R: RngLike> Vm<R> {
    /// Discard every typed side-channel parked for the next exception funnel
    /// conversion. A carrier is owned by one raise only; retaining it after
    /// preserving an already-materialized exception would let a later error
    /// observe stale fields (Issue #11632).
    pub(in crate::vm) fn clear_pending_exception_payloads(&mut self) {
        self.pending_exception_payload.clear();
    }

    fn exception_error_with_payload(&mut self, payload: PendingExceptionPayload) -> VmError {
        self.pending_exception_payload.park_and_construct(payload)
    }

    fn attach_exception_payload(&mut self, payload: PendingExceptionPayload, err: &VmError) {
        self.pending_exception_payload
            .park_for_existing(payload, err);
    }

    fn error_exception_value(&mut self, msg: &str) -> Option<Value> {
        use crate::vm::value::StructInstance;
        let type_id = self
            .struct_defs
            .iter()
            .position(|d| d.name == "ErrorException")?;
        let instance = StructInstance::with_name(
            type_id,
            "ErrorException".to_string(),
            vec![Value::str_new(msg.to_string())],
        );
        let idx = self.struct_heap.len();
        self.struct_heap.push(instance);
        Some(Value::StructRef(idx))
    }

    /// Whether a [`VmError`] represents a Julia-level exception that user code
    /// may observe with `try`/`catch` (Issue #10406).
    ///
    /// Now a thin delegation to the exception-type funnel
    /// ([`VmError::is_catchable`], Issue #11146): an error is catchable exactly
    /// when the funnel assigns it a Julia exception class, i.e. exactly when
    /// [`Self::vm_error_to_exception_value`] can build an exception object for
    /// it. The two used to be a hand-synced pair of variant lists carrying an
    /// "INVARIANT: keep byte-for-byte in sync" comment; a drift silently made a
    /// variant catchable-but-bound-as-a-`String` (or the reverse). Both are now
    /// derived from the one match in `subset_julia_vm_bytecode/src/error.rs`,
    /// so they cannot disagree.
    ///
    /// The run loop's terminal error arm uses this to route errors that an
    /// instruction handler propagated with a bare `?` (instead of `self.raise`)
    /// through the same handler machinery, so an enclosing `try`/`catch` can
    /// observe them (Issue #10406).
    pub(in crate::vm) fn is_catchable_vm_error(err: &VmError) -> bool {
        err.is_catchable()
    }

    /// Reconstruct the `Exception` struct for a Rust-raised [`VmError`] so a
    /// `catch` binds a value with the correct type (Issue #5648, #8643, #8664).
    ///
    /// The exception's TYPE is no longer chosen here: it comes from the
    /// taxonomy funnel ([`VmError::exception_class`], Issue #11146), so this
    /// function only supplies each class's FIELD values. A raise site therefore
    /// cannot cause a `catch` to bind an object whose class contradicts the
    /// variant it raised — the class is derived, not re-decided (before #11146
    /// every arm here hard-coded its own struct-name string literal, and four of
    /// the five root causes in Issue #10354's fixture-fallout measurement were
    /// exactly that contradiction).
    ///
    /// Return value:
    /// - `Some(StructRef)` — a heap-allocated exception struct of the funnel's
    ///   class; `typeof(e)` / `e isa Exception` behave like upstream.
    /// - `None` — only for [`ExceptionClass::VmInternal`] errors (uncatchable by
    ///   construction, see [`Self::is_catchable_vm_error`]), or if the class's
    ///   struct is somehow absent from Base — a hole pinned shut by
    ///   `exception_class_julia_names_resolve_in_base_11146`.
    ///
    /// Field values the `VmError` does not carry are filled with `Nothing`
    /// placeholders — `typeof(e)`, `e isa Exception`, and the message field are
    /// what callers primarily rely on (Issue #8212 / PR #8228 precedent).
    /// Build a `no method matching name(...)` MethodError and park its typed
    /// payload (callable + argument values) for the funnel, so a caught
    /// error exposes upstream's `.f`/`.args` (Issue #11374). The payload is
    /// keyed by the exact message; the funnel consumes it unconditionally so
    /// it can never outlive its raise.
    pub(in crate::vm) fn method_error_with_payload(
        &mut self,
        message: String,
        f_name: &str,
        args: &[Value],
    ) -> VmError {
        self.exception_error_with_payload(PendingExceptionPayload::method_error(
            message, f_name, args,
        ))
    }

    /// Attach the typed `.f`/`.args` payload to an already-built
    /// `MethodError` (fast-path remaps that construct the error away from
    /// `&mut self`, Issue #11374). No-op for other error classes.
    pub(in crate::vm) fn park_method_error_payload(
        &mut self,
        err: &VmError,
        f_name: &str,
        args: &[Value],
    ) {
        if let VmError::MethodError(message) = err {
            self.attach_exception_payload(
                PendingExceptionPayload::method_error(message.clone(), f_name, args),
                err,
            );
        }
    }

    /// Raise a `DomainError(val, msg)` whose caught `.val` is the actual
    /// out-of-domain value instead of a `nothing` placeholder (Issue #11399).
    /// The value is parked keyed by the message; the funnel consumes it
    /// unconditionally so it can never outlive its raise.
    pub(in crate::vm) fn domain_error_with_val(&mut self, msg: String, val: Value) -> VmError {
        self.exception_error_with_payload(PendingExceptionPayload::Domain { message: msg, val })
    }

    /// Raise a `TypeError(func, context, expected, got)` whose caught fields are
    /// the real typed payload instead of `:unknown`/`nothing` placeholders
    /// (Issue #11399). Message-keyed; the funnel consumes it unconditionally.
    pub(in crate::vm) fn type_error_with_payload(
        &mut self,
        msg: String,
        func: Value,
        context: Value,
        expected: Value,
        got: Value,
    ) -> VmError {
        self.exception_error_with_payload(PendingExceptionPayload::Type {
            message: msg,
            func,
            context,
            expected,
            got,
        })
    }

    /// Raise a `StringIndexError(string, index)` whose caught `.string` is the
    /// exact runtime value, including byte-backed invalid-UTF-8 strings
    /// (Issue #11572). Parking and error construction are one operation, so a
    /// successful index cannot leave stale payload state behind.
    pub(in crate::vm) fn string_index_error_with_string(
        &mut self,
        string: Value,
        index: i64,
        valid_indices: (i64, i64),
    ) -> VmError {
        self.exception_error_with_payload(PendingExceptionPayload::StringIndex {
            index,
            valid_indices,
            string,
        })
    }

    /// Raise a parser-originated `ParseError(msg, detail)` with the structured
    /// JuliaSyntax detail parked for the exception funnel (Issue #11572).
    /// Message-keyed and atomic with error construction.
    pub(in crate::vm) fn parse_error_with_detail(
        &mut self,
        message: String,
        detail: Value,
    ) -> VmError {
        self.exception_error_with_payload(PendingExceptionPayload::Parse { message, detail })
    }

    /// Raise a `getfield`/`_getfield` out-of-range-index `FieldIndexOutOfBounds`
    /// whose caught `BoundsError` exposes the actual receiver instead of a
    /// `nothing` placeholder (Issue #11509). Parks the receiver keyed by the
    /// exact `(index, field_count)` pair being raised, atomically with
    /// constructing the error — mirroring `domain_error_with_val`/
    /// `type_error_with_payload` — so a stale receiver can never be parked
    /// ahead of a field lookup that might succeed and never actually raise.
    /// (A previous version parked unconditionally before the lookup; a
    /// successful getfield then left a stale receiver behind that could
    /// attach to a later, unrelated `FieldIndexOutOfBounds` raised through a
    /// path that never parks one, e.g. `setfield!` with an out-of-range
    /// index — the non-transactional side-channel bug class from Issue
    /// #9787.)
    pub(in crate::vm) fn field_index_out_of_bounds_with_receiver(
        &mut self,
        index: usize,
        field_count: usize,
        receiver: Value,
    ) -> VmError {
        self.exception_error_with_payload(PendingExceptionPayload::FieldIndex {
            index,
            field_count,
            receiver,
        })
    }

    pub(in crate::vm) fn vm_error_to_exception_value(&mut self, err: &VmError) -> Option<Value> {
        use crate::vm::value::{StructInstance, SymbolValue, TupleValue};
        // Consume the single typed carrier before classification. A mismatch or
        // an internal error discards it just as definitively as a successful
        // conversion (Issue #11647).
        let pending_fields = self.pending_exception_payload.take_fields_for(err);
        // The funnel decides the class; a VM-internal error has no Julia
        // exception object at all and stays uncatchable.
        let name = err.exception_class().julia_name()?;

        let fields: Vec<Value> = match err {
            VmError::DivisionByZero | VmError::OutOfMemory | VmError::StackOverflow => vec![],
            // Upstream array bounds errors carry the complete index tuple
            // (`A[10]` reports `.i == (10,)`, Issue #11374).
            VmError::IndexOutOfBounds { indices, .. } => vec![
                Value::Nothing,
                Value::Tuple(TupleValue::new(
                    indices.iter().map(|&index| Value::I64(index)).collect(),
                )),
            ],
            VmError::RangeIndexOutOfBounds { index, .. }
            | VmError::TupleIndexOutOfBounds { index, .. } => {
                vec![Value::Nothing, Value::I64(*index)]
            }
            // MethodError: the parked typed payload supplies upstream's
            // `.f`/`.args` when this exact error raised it (Issue #11374);
            // otherwise args=empty tuple prevents `_showerror_str` from
            // crashing on `length(nothing)` (Issue #8748).
            VmError::MethodError(msg) => pending_fields.unwrap_or_else(|| {
                vec![
                    Value::str_new(msg.clone()),
                    Value::Tuple(TupleValue::new(vec![])),
                ]
            }),
            VmError::DictKeyNotFound(key) | VmError::InvalidDictKey(key) => {
                vec![Value::str_new(key.clone())]
            }
            // Issues #8212/#8732: reconstruct the Julia 1.12
            // `InexactError(func, args)` struct from the carried
            // `"{T}({val})"` message.
            VmError::InexactError(msg) => inexact_error_fields(msg),
            // DomainError: the parked out-of-domain value supplies upstream's
            // `.val` when this exact error raised it (Issue #11399); otherwise
            // val=Nothing placeholder (showerror shows "with nothing:").
            VmError::DomainError(msg) => {
                pending_fields.unwrap_or_else(|| vec![Value::Nothing, Value::str_new(msg.clone())])
            }
            VmError::OverflowError(msg)
            | VmError::ErrorException(msg)
            | VmError::ArgumentError(msg)
            | VmError::AssertionFailed(msg)
            | VmError::DimensionMismatchMsg(msg) => vec![Value::str_new(msg.clone())],
            // `NotImplemented` surfaces as `ErrorException` (Issue #11146; see
            // the funnel's arm for why #8664's `None` was revisited). The
            // message keeps the "Feature not implemented: ..." wording via
            // Display, so the gap stays loud — it is now merely a *typed*,
            // catchable failure instead of a raw `String`.
            VmError::NotImplemented(_) => vec![Value::str_new(err.to_string())],
            VmError::ParseError(msg) => {
                pending_fields.unwrap_or_else(|| vec![Value::str_new(msg.clone()), Value::Nothing])
            }
            VmError::DimensionMismatch { expected, got } => {
                vec![Value::str_new(format!(
                    "a has dimensions {}, b has dimensions {}",
                    expected, got
                ))]
            }
            VmError::MatMulDimensionMismatch { a_shape, b_shape } => {
                vec![Value::str_new(format!(
                    "incompatible dimensions for matrix multiplication: {:?} * {:?}",
                    a_shape, b_shape
                ))]
            }
            VmError::BroadcastDimensionMismatch { a_shape, b_shape } => {
                vec![Value::str_new(format!(
                    "arrays could not be broadcast to a common size: {:?} vs {:?}",
                    a_shape, b_shape
                ))]
            }
            // Upstream ArgumentError messages for the empty-collection guards.
            VmError::EmptyArrayPop => vec![Value::str_new("array must be non-empty".to_string())],
            VmError::EmptyRange => vec![Value::str_new("range must be non-empty".to_string())],
            VmError::EmptyTuple => vec![Value::str_new("tuple must be non-empty".to_string())],
            // TypeError: the parked typed payload supplies upstream's
            // `.func`/`.context`/`.expected`/`.got` when this exact error
            // raised it (Issue #11399); otherwise fill with placeholders so
            // typeof(e) == TypeError.
            VmError::TypeError(msg) => pending_fields.unwrap_or_else(|| {
                vec![
                    Value::Symbol(SymbolValue::new(":unknown")),
                    Value::str_new(msg.clone()),
                    Value::Nothing,
                    Value::Nothing,
                ]
            }),
            // Upstream's `struct UndefRefError <: Exception end` is fieldless.
            VmError::UndefRefError => vec![],
            // FieldError (Julia 1.12, Issue #10067): the field does not exist on
            // the type at all.
            VmError::FieldError { type_name, field } => vec![
                Value::DataType(Box::new(crate::types::JuliaType::from_name_or_struct(
                    type_name,
                ))),
                Value::Symbol(SymbolValue::new(field.as_str())),
            ],
            VmError::NamedTupleFieldNotFound(field) => vec![
                Value::DataType(Box::new(crate::types::JuliaType::from_name_or_struct(
                    "NamedTuple",
                ))),
                Value::Symbol(SymbolValue::new(field.as_str())),
            ],
            // Out-of-range numeric getfield/setfield is an upstream
            // BoundsError. `.a` is the parked receiver when this exact
            // `(index, field_count)` raised it (Issue #11509) — keyed by the
            // shared carrier so a receiver parked for a different raise (or a
            // stale one left by a bug) can never attach here — and `.i` is the caller's
            // original 1-based index, not the internal 0-based `field_idx`.
            // Sites that raise `FieldIndexOutOfBounds` without going through
            // `field_index_out_of_bounds_with_receiver` (e.g. `setfield!`,
            // Issue #11509 tracks `getfield` only) fall back to `Nothing`,
            // matching this conversion's pre-#11509 behavior for those sites.
            VmError::FieldIndexOutOfBounds { index, .. } => {
                pending_fields.unwrap_or_else(|| vec![Value::Nothing, Value::I64(*index as i64)])
            }
            // Upstream raises BoundsError on the tuple access that exceeds its length.
            VmError::TupleDestructuringMismatch { got, .. } => {
                vec![Value::Nothing, Value::I64(*got as i64)]
            }
            VmError::ImmutableFieldAssign(name) => vec![Value::str_new(format!(
                "setfield!: immutable struct of type {} cannot be changed",
                name
            ))],
            VmError::NamedTupleLengthMismatch { .. } => vec![Value::str_new(
                "NamedTuple names and field types must have matching lengths".to_string(),
            )],
            // UndefVarError: var is a Symbol; scope is Nothing when bare, or the
            // qualified module string (Issue #10318) so `_showerror_str` renders
            // `not defined in `Main.<Module>``.
            VmError::UndefVarError(name) => vec![
                Value::Symbol(SymbolValue::new(name.as_str())),
                Value::Nothing,
            ],
            VmError::UndefVarErrorInModule { var, scope } => vec![
                Value::Symbol(SymbolValue::new(var.as_str())),
                Value::str_new(scope.clone()),
            ],
            VmError::UndefKeywordError(name) => {
                vec![Value::Symbol(SymbolValue::new(name.as_str()))]
            }
            // Upstream struct has (string, index). Runtime producers park the
            // exact String/StrBytes receiver; synthetic errors retain the
            // historical empty-string fallback (Issue #11572).
            VmError::StringIndexError {
                index,
                valid_indices: _,
            } => pending_fields
                .unwrap_or_else(|| vec![Value::str_new(String::new()), Value::I64(*index)]),

            // VM-internal errors: unreachable here — the funnel gives them no
            // `julia_name()`, so the `?` above already returned `None`. The arm
            // exists so this match stays exhaustive (a new variant must be
            // classified in the funnel AND given fields here).
            VmError::Cancelled
            | VmError::StackUnderflow
            | VmError::InternalError(_)
            | VmError::UnknownBroadcastOp(_)
            | VmError::InvalidInstruction => vec![],
        };
        let type_id = self.struct_defs.iter().position(|d| d.name == name)?;
        let instance = StructInstance::with_name(type_id, name.to_string(), fields);
        let idx = self.struct_heap.len();
        self.struct_heap.push(instance);
        Some(Value::StructRef(idx))
    }

    /// Execute error handling and test instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_error_handling(
        &mut self,
        instr: &Instr,
    ) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::ThrowError => {
                let msg = match self.stack.pop() {
                    Some(Value::Str(s)) => s.to_string(),
                    Some(v) => format!("{:?}", v),
                    None => "error".to_string(),
                };
                self.raise(VmError::ErrorException(msg))?;
                Ok(DispatchAction::Continue)
            }

            Instr::ThrowMethodError(msg) => {
                // Ambiguous-dispatch MethodError raised at runtime (Issue #5071).
                // The message is carried inline (known at compile time), so this
                // raises a catchable `VmError::MethodError` directly — matching
                // upstream Julia, which raises an ambiguity `MethodError` at
                // runtime rather than aborting compilation.
                self.raise(VmError::MethodError(msg.clone()))?;
                Ok(DispatchAction::Continue)
            }

            Instr::ThrowUndefVarError(name) => {
                // Calling a name that resolves to no function/method/builtin
                // anywhere (Issue #10354's fixture-fallout measurement,
                // `modules/module_selective_using_globals_7955.jl`; see
                // docs/vm/EXCEPTION_PARITY.md). The name is known at compile
                // time and carried inline, mirroring `ThrowMethodError` above,
                // so this raises the upstream-matching `VmError::UndefVarError`
                // directly instead of the generic `ErrorException` every other
                // unresolved-name message used to fall back to via
                // `ThrowError`. Reading the same undefined name (not calling
                // it) already raised `UndefVarError` via the ordinary global
                // load path; this closes the gap that was specific to the
                // call position.
                self.raise(VmError::UndefVarError(name.clone()))?;
                Ok(DispatchAction::Continue)
            }

            Instr::RaiseUndefVarErrorIfFunctionInvisible(name) => {
                // Emitted before a keyword-free splat/positional dynamic call
                // evaluates its arguments (Issue #11320): a hoisted-but-not-
                // yet-activated callee must be reported UndefVarError before
                // any argument expression's side effects run, matching
                // upstream's callee-before-arguments evaluation order. A
                // no-op for any other callee shape (genuinely undeclared
                // name, builtin, or an already-visible generic function).
                if self.function_name_exists_only_as_unactivated(name) {
                    self.raise(VmError::UndefVarError(name.clone()))?;
                }
                Ok(DispatchAction::Continue)
            }

            Instr::ThrowValue => {
                // Pop any value (typically an exception struct) and throw it
                // This preserves the original value so it can be accessed in catch blocks
                let value = self.stack.pop().ok_or(VmError::StackUnderflow)?;

                // Store the exception value for later retrieval in catch blocks
                self.pending_exception_value = Some(value.clone());

                // Resolve StructRef to Struct for proper formatting
                let resolved = if let Value::StructRef(idx) = &value {
                    if let Some(s) = self.struct_heap.get(*idx) {
                        Value::Struct(s.clone())
                    } else {
                        value.clone()
                    }
                } else {
                    value.clone()
                };

                // Create an error message from the value for the VmError.
                // Recognized exception structs are formatted with their
                // upstream `showerror` message rather than the raw struct
                // constructor repr (Issue #5146).
                let msg = match &resolved {
                    Value::Struct(_) => {
                        self.format_exception_struct(&resolved).unwrap_or_else(|| {
                            format_value(&Resolved::new(&resolved, &self.struct_heap))
                        })
                    }
                    Value::Str(s) => s.to_string(),
                    _ => format!("{:?}", resolved),
                };

                self.raise(VmError::ErrorException(msg))?;
                Ok(DispatchAction::Continue)
            }

            Instr::PushExceptionValue => {
                // Push the pending exception value onto the stack so catch blocks
                // can access the original exception. A pure-Julia `throw(E(...))`
                // already stored the struct in `pending_exception_value`. A
                // Rust-raised `VmError` (e.g. `sqrt(-1)`'s DomainError, an out-of-
                // bounds BoundsError, a DivideError) did not — those previously
                // surfaced as a bare `String`, losing `typeof(e)` (Issue #5648).
                // Reconstruct the matching `Exception` struct so `typeof(e)` /
                // `e isa Exception` / field access behave like upstream.
                let value = if let Some(v) = self.pending_exception_value.clone() {
                    v
                } else if let Some(err) = self.pending_error.clone() {
                    self.vm_error_to_exception_value(&err)
                        .unwrap_or_else(|| Value::str_new(err.to_string()))
                } else {
                    Value::str_new("Unknown error".to_string())
                };
                self.stack.push(value);
                Ok(DispatchAction::Continue)
            }

            Instr::Test(msg) => {
                let v = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let cond = match v {
                    Value::Bool(b) => b,
                    _ => {
                        let type_name = self.get_type_name(&v);
                        // User-visible: user can pass a non-boolean expression to @test or as an if-condition
                        return Err(VmError::TypeError(format!(
                            "non-boolean ({}) used in boolean context",
                            type_name
                        )));
                    }
                };
                if cond {
                    self.test_pass_count += 1;
                    let prefix = if let Some(ref ts) = self.current_testset {
                        format!("  [{}] ", ts)
                    } else {
                        "  ".to_string()
                    };
                    if msg.is_empty() {
                        self.emit_output(&format!("{}Test Passed", prefix), true);
                    } else {
                        self.emit_output(&format!("{}Test Passed: {}", prefix, msg), true);
                    }
                } else {
                    self.test_fail_count += 1;
                    self.any_test_failed = true; // Issue #8191: drives non-zero CLI exit.
                    let prefix = if let Some(ref ts) = self.current_testset {
                        format!("  [{}] ", ts)
                    } else {
                        "  ".to_string()
                    };
                    if msg.is_empty() {
                        self.emit_output(&format!("{}Test Failed", prefix), true);
                    } else {
                        self.emit_output(&format!("{}Test Failed: {}", prefix, msg), true);
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::TestSetBegin(name) => {
                // Shared frame push with the `_testset_begin!` builtin lane so
                // nested testsets aggregate identically (Issue #10338).
                self.testset_begin_frame(name.clone());
                self.emit_output(&format!("Test Set: {}", name), true);
                self.emit_output(&"=".repeat(40), true);
                Ok(DispatchAction::Continue)
            }

            Instr::TestSetEnd => {
                let (pass, fail, _errored, _broken) = self.testset_end_frame();
                let total = pass + fail;
                self.emit_output(&"-".repeat(40), true);
                self.emit_output(
                    &format!(
                        "Results: {} passed, {} failed (total: {})",
                        pass, fail, total
                    ),
                    true,
                );
                if fail == 0 {
                    self.emit_output("All tests passed!", true);
                }
                self.emit_output("", true);
                Ok(DispatchAction::Continue)
            }

            Instr::TestThrowsBegin(expected_type) => {
                // Initialize test_throws state - we expect this exception type
                self.test_throws_state = Some((expected_type.clone(), false));
                Ok(DispatchAction::Continue)
            }

            Instr::TestThrowsEnd => {
                // Check if exception was thrown as expected
                if let Some((expected_type, was_thrown)) = self.test_throws_state.take() {
                    if was_thrown {
                        // Pass: exception was thrown
                        self.test_pass_count += 1;
                        self.emit_output(
                            &format!(
                                "  Test Passed: @test_throws {} (exception was thrown)",
                                expected_type
                            ),
                            true,
                        );
                    } else {
                        // Fail: no exception was thrown
                        self.test_fail_count += 1;
                        self.any_test_failed = true; // Issue #8191: drives non-zero CLI exit.
                        self.emit_output(
                            &format!(
                                "  Test Failed: @test_throws {} (no exception was thrown)",
                                expected_type
                            ),
                            true,
                        );
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::PushHandler(catch_ip, finally_ip) => {
                let handler = Handler {
                    catch_ip: *catch_ip,
                    finally_ip: *finally_ip,
                    stack_len: self.stack.len(),
                    frame_len: self.frames.len(),
                    return_ip_len: self.return_ips.len(),
                    lexical_scope_len: self.lexical_scopes.len(),
                    caught_exception_len: self.caught_exceptions.len(),
                    finally_pending_len: self.pending_finally_rethrows.len(),
                };
                self.handlers.push(handler);
                Ok(DispatchAction::Continue)
            }

            Instr::PopHandler => {
                self.handlers.pop();
                Ok(DispatchAction::Continue)
            }

            Instr::ClearError => {
                if let Some(err) = self.pending_error.take() {
                    let value = self.pending_exception_value.take();
                    let backtrace = self.pending_backtrace.take().unwrap_or_default();
                    self.caught_exceptions.push((err, value, backtrace));
                } else {
                    self.pending_exception_value = None;
                    self.pending_backtrace = None;
                }
                // Note: this catch clause's own pending-finally-rethrow marker
                // (if any) was already truncated away by `handle_error` when it
                // routed here (Issue #11306). Do NOT touch
                // `pending_finally_rethrows` further here — an *enclosing*
                // finally's marker must survive a nested catch that merely
                // handles its own, unrelated, `rethrow()`.
                // Mark exception as thrown for @test_throws context
                if let Some((_, ref mut was_thrown)) = self.test_throws_state {
                    *was_thrown = true;
                }
                Ok(DispatchAction::Continue)
            }

            Instr::PopCaughtException => {
                self.caught_exceptions.pop();
                Ok(DispatchAction::Continue)
            }

            Instr::PushErrorCode => {
                let code = self
                    .pending_error
                    .as_ref()
                    .map(Self::error_code)
                    .unwrap_or(0);
                self.stack.push(Value::I64(code));
                Ok(DispatchAction::Continue)
            }

            Instr::PushErrorMessage => {
                let message = self
                    .pending_error
                    .as_ref()
                    .map(|err| err.to_string())
                    .unwrap_or_default();
                self.stack.push(Value::str_new(message));
                Ok(DispatchAction::Continue)
            }

            Instr::Rethrow => {
                // Pop THIS finally instance's own marker (Issue #11306): a
                // nested try/catch inside the finally body cannot have
                // consumed it (see `Handler::finally_pending_len` /
                // `handle_error`), so if the stack has an entry here it is
                // exactly the exception whose unwind entered this finally,
                // still needing to reach the enclosing handler.
                if let Some((err, value, backtrace)) = self.pending_finally_rethrows.pop() {
                    self.pending_exception_value = value;
                    self.pending_backtrace = backtrace;
                    self.raise(err)?;
                }
                Ok(DispatchAction::Continue)
            }

            Instr::RethrowCurrent => {
                // Julia's rethrow() - rethrow the current pending exception
                if let Some((err, value, backtrace)) = self.caught_exceptions.pop() {
                    self.pending_exception_value = value;
                    self.pending_backtrace = Some(backtrace);
                    self.raise(err)?;
                    Ok(DispatchAction::Continue)
                } else if let Some(err) = self.pending_error.take() {
                    // Preserve the exception value for later catch blocks
                    self.raise(err)?;
                    Ok(DispatchAction::Continue)
                } else {
                    // No pending exception - this is an error in Julia, and it
                    // is catchable by an enclosing try block.
                    let msg = "rethrow() not allowed outside a catch block";
                    self.pending_exception_value = self.error_exception_value(msg);
                    self.raise(VmError::ErrorException(msg.to_string()))?;
                    Ok(DispatchAction::Continue)
                }
            }

            Instr::RethrowOther => {
                // Julia's rethrow(e) - rethrow with a different exception value
                let value = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                if self.caught_exceptions.pop().is_none() && self.pending_error.is_none() {
                    let msg = "rethrow(exc) not allowed outside a catch block";
                    self.pending_exception_value = self.error_exception_value(msg);
                    self.raise(VmError::ErrorException(msg.to_string()))?;
                    return Ok(DispatchAction::Continue);
                }
                self.pending_backtrace = None;

                // Store the new exception value
                self.pending_exception_value = Some(value.clone());

                // Resolve StructRef to Struct for proper formatting
                let resolved = if let Value::StructRef(idx) = &value {
                    if let Some(s) = self.struct_heap.get(*idx) {
                        Value::Struct(s.clone())
                    } else {
                        value.clone()
                    }
                } else {
                    value.clone()
                };

                // Create an error message from the value (Issue #5146).
                let msg = match &resolved {
                    Value::Struct(_) => {
                        self.format_exception_struct(&resolved).unwrap_or_else(|| {
                            format_value(&Resolved::new(&resolved, &self.struct_heap))
                        })
                    }
                    Value::Str(s) => s.to_string(),
                    _ => format!("{:?}", resolved),
                };

                self.raise(VmError::ErrorException(msg))?;
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }

    /// Format a recognized exception struct using its upstream `showerror`
    /// message, returning `None` for structs we do not special-case (so the
    /// caller falls back to the raw `format_value` repr).
    ///
    /// Currently handles `TypeError` (Issue #5146), mirroring
    /// `julia/base/errorshow.jl`'s `showerror(io, ex::TypeError)` and the pure
    /// Julia `_showerror_str(ex::TypeError)` in `base/errorshow.jl`. `TypeError`
    /// fields are `(func, context, expected, got)`; `got` holds the offending
    /// VALUE, formatted as "a value of type $(typeof(got))".
    fn format_exception_struct(&self, value: &Value) -> Option<String> {
        let s = match value {
            Value::Struct(s) => s,
            _ => return None,
        };
        if &*s.struct_name != "TypeError" || s.values.len() != 4 {
            return None;
        }
        // `func`/`context` interpolate as bare names (e.g. `typeassert`, not
        // `:typeassert`); `format_value_print` strips the leading `:` for
        // Symbol, matching upstream `string(ex.func)`.
        let resolved = resolve_struct_refs_for_format(&s.values[0], &self.struct_heap);
        let func = format_value_print(&Resolved::trivial(&resolved));
        let resolved = resolve_struct_refs_for_format(&s.values[1], &self.struct_heap);
        let context = match &resolved {
            Value::Str(c) => c.to_string(),
            // `other` is borrowed from the heap-resolved field above, so it holds
            // no StructRef → `Resolved::trivial` (Issue #6421 / #8642).
            other => format_value_print(&Resolved::trivial(other)),
        };
        let resolved = resolve_struct_refs_for_format(&s.values[2], &self.struct_heap);
        let expected = format_value(&Resolved::trivial(&resolved));
        let got = resolve_struct_refs_for_format(&s.values[3], &self.struct_heap);

        // `expected === Bool` => non-boolean message.
        if expected == "Bool" {
            return Some(format!(
                "TypeError: non-boolean ({}) used in boolean context",
                self.get_type_name(&got)
            ));
        }

        // A value that is itself a type prints as `Type{T}`; otherwise as
        // "a value of type $(typeof(got))".
        let targ = match &got {
            // `got` was heap-resolved before this match, so it holds no StructRef
            // → `Resolved::trivial` (Issue #6421 / #8642).
            Value::DataType(_) => format!("Type{{{}}}", format_value(&Resolved::trivial(&got))),
            _ => format!("a value of type {}", self.get_type_name(&got)),
        };
        let ctx = if context.is_empty() {
            format!("in {}", func)
        } else {
            format!("in {}, in {}", func, context)
        };
        Some(format!(
            "TypeError: {}, expected {}, got {}",
            ctx, expected, targ
        ))
    }
}
