//! I/O builtin functions for the VM.
//!
//! Print, IOBuffer, and time operations.

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::formatting::Resolved;
use super::hof_exec::state::{RedirectState, RedirectStreamKind, RuntimeCallableResult};
use super::stack_ops::StackOps;
use super::value::{
    native_array_value_from_array, ArrayValue, IOKind, IORef, IOValue, TupleValue, Value,
};
use super::Vm;

fn read_text_file(path: &str, operation: &str) -> Result<String, VmError> {
    crate::include::read_include_file(std::path::Path::new(path)).map_err(|e| {
        VmError::ErrorException(format!(
            "{operation}: failed to read file '{}': {}",
            path, e
        ))
    })
}

fn is_iocontext_struct_name(name: &str) -> bool {
    let head = name.split_once('{').map_or(name, |(head, _)| head);
    head == "IOContext" || head.ends_with(".IOContext")
}

fn iowrite_payload_bytes(value: &Value) -> Vec<u8> {
    match value {
        Value::I8(n) => n.to_le_bytes().to_vec(),
        Value::I16(n) => n.to_le_bytes().to_vec(),
        Value::I32(n) => n.to_le_bytes().to_vec(),
        Value::I64(n) => n.to_le_bytes().to_vec(),
        Value::I128(n) => n.to_le_bytes().to_vec(),
        Value::U8(n) => n.to_le_bytes().to_vec(),
        Value::U16(n) => n.to_le_bytes().to_vec(),
        Value::U32(n) => n.to_le_bytes().to_vec(),
        Value::U64(n) => n.to_le_bytes().to_vec(),
        Value::U128(n) => n.to_le_bytes().to_vec(),
        Value::Bool(b) => vec![u8::from(*b)],
        Value::F16(n) => n.to_bits().to_le_bytes().to_vec(),
        Value::F32(n) => n.to_le_bytes().to_vec(),
        Value::F64(n) => n.to_le_bytes().to_vec(),
        Value::Char(ch) => {
            let mut buf = [0_u8; 4];
            ch.encode_utf8(&mut buf).as_bytes().to_vec()
        }
        // Malformed Char (Issue #8995): write the raw pattern bytes, matching
        // upstream `write(io, '\xff')` emitting the byte 0xff.
        Value::CharMalformed(bits) => {
            let (bytes, len) = crate::vm::value::julia_char_pattern_bytes(*bits);
            bytes[..len].to_vec()
        }
        Value::Str(s) => s.as_bytes().to_vec(),
        Value::StrBytes(bytes) => bytes.to_vec(),
        _ => crate::vm::formatting::format_value_print(&Resolved::trivial(value)).into_bytes(),
    }
}

/// Raw print bytes for values that would otherwise lose invalid UTF-8 through
/// the lossy String render (Issue #8995): only the invalid carriers — a
/// byte-backed String or a malformed Char. Valid strings/chars keep the
/// normal (identical) text path.
fn invalid_utf8_print_bytes(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::StrBytes(bytes) => Some(bytes.as_ref().to_vec()),
        Value::CharMalformed(bits) => {
            let (bytes, len) = crate::vm::value::julia_char_pattern_bytes(*bits);
            Some(bytes[..len].to_vec())
        }
        _ => None,
    }
}

impl<R: RngLike> Vm<R> {
    fn redirect_stream_slot(&mut self, kind: RedirectStreamKind) -> &mut IORef {
        match kind {
            RedirectStreamKind::Stdout => &mut self.current_stdout,
            RedirectStreamKind::Stderr => &mut self.current_stderr,
        }
    }

    fn set_redirect_stream(&mut self, kind: RedirectStreamKind, stream: IORef) -> IORef {
        let slot = self.redirect_stream_slot(kind);
        std::mem::replace(slot, stream)
    }

    pub(in crate::vm) fn restore_redirect_stream(&mut self, state: RedirectState) {
        let slot = self.redirect_stream_slot(state.kind);
        *slot = state.old_stream;
    }

    fn execute_redirect_stdio(
        &mut self,
        kind: RedirectStreamKind,
        argc: usize,
    ) -> Result<(), VmError> {
        match argc {
            1 => {
                let stream = self.stack.pop_value()?;
                let Value::IO(stream_ref) = stream else {
                    return Err(VmError::TypeError(
                        "redirect stdio requires an IO stream".to_string(),
                    ));
                };
                self.set_redirect_stream(kind, stream_ref.clone());
                self.stack.push(Value::IO(stream_ref));
                Ok(())
            }
            2 => {
                let stream = self.stack.pop_value()?;
                let thunk = self.stack.pop_value()?;
                let Value::IO(stream_ref) = stream else {
                    return Err(VmError::TypeError(
                        "redirect stdio requires an IO stream".to_string(),
                    ));
                };

                let old_stream = self.set_redirect_stream(kind, stream_ref);
                let state = RedirectState {
                    kind,
                    old_stream,
                    call_frame_depth: self.frames.len(),
                };
                self.redirect_states.push(state);

                match self.call_runtime_callable_value(thunk, Vec::new()) {
                    Ok(RuntimeCallableResult::StartedFrame) => Ok(()),
                    Ok(RuntimeCallableResult::Immediate(value)) => {
                        if let Some(state) = self.redirect_states.pop() {
                            self.restore_redirect_stream(state);
                        }
                        self.stack.push(value);
                        Ok(())
                    }
                    Ok(RuntimeCallableResult::Raised) => {
                        if let Some(state) = self.redirect_states.pop() {
                            self.restore_redirect_stream(state);
                        }
                        Ok(())
                    }
                    Err(err) => {
                        if let Some(state) = self.redirect_states.pop() {
                            self.restore_redirect_stream(state);
                        }
                        Err(err)
                    }
                }
            }
            _ => Err(VmError::TypeError(
                "redirect stdio expects one stream or a thunk plus stream".to_string(),
            )),
        }
    }

    pub(in crate::vm) fn handle_redirect_return(&mut self, result: Value) -> Result<bool, VmError> {
        let should_handle = self
            .redirect_states
            .last()
            .map(|state| self.frames.len() == state.call_frame_depth + 1)
            .unwrap_or(false);
        if !should_handle {
            return Ok(false);
        }

        let state = self.redirect_states.pop().ok_or_else(|| {
            VmError::InternalError("redirect state disappeared during return".to_string())
        })?;
        self.restore_redirect_stream(state);

        let Some(return_ip) = self.return_ips.pop() else {
            return Err(VmError::InternalError(
                "redirect thunk returned without a return IP".to_string(),
            ));
        };
        self.pop_handlers_for_return();
        self.pop_call_frame();
        self.ip = return_ip;
        self.stack.push(result);
        Ok(true)
    }

    fn pop_builtin_values(&mut self, argc: usize) -> Result<Vec<Value>, VmError> {
        let mut values = Vec::with_capacity(argc);
        for _ in 0..argc {
            values.push(self.stack.pop_value()?);
        }
        values.reverse();
        Ok(values)
    }

    fn render_print_value(&mut self, value: &Value) -> String {
        let resolved =
            crate::vm::formatting::resolve_struct_refs_for_format(value, &self.struct_heap);
        self.render_value_via_user_show_for_print(&resolved)
            .or_else(|| self.render_array_via_user_show(&resolved))
            .unwrap_or_else(|| {
                crate::vm::formatting::format_value_print(&Resolved::trivial(&resolved))
            })
    }

    fn print_io_sink_value(&self, value: &Value) -> Option<Value> {
        match value {
            Value::IO(_) => Some(value.clone()),
            Value::Struct(instance) if is_iocontext_struct_name(&instance.struct_name) => instance
                .values
                .first()
                .and_then(|inner| self.print_io_sink_value(inner)),
            Value::StructRef(idx) => {
                let instance = self.struct_heap.get(*idx)?;
                if !is_iocontext_struct_name(&instance.struct_name) {
                    return None;
                }
                instance
                    .values
                    .first()
                    .and_then(|inner| self.print_io_sink_value(inner))
            }
            _ => None,
        }
    }

    pub(crate) fn emit_print_text_to_sink(
        &mut self,
        sink: Option<&Value>,
        text: &str,
    ) -> Result<(), VmError> {
        let resolved_sink = sink.and_then(|value| self.print_io_sink_value(value));
        if let Some(Value::IO(io_ref)) = resolved_sink.as_ref() {
            let io_kind = io_ref.borrow().kind.clone();
            if io_kind == IOKind::Buffer {
                io_ref.borrow_mut().write_buffer_str(text).map_err(|e| {
                    VmError::ErrorException(format!("print: error writing to IOBuffer: {}", e))
                })?;
            } else if self.sprint_state.is_some() || io_kind == IOKind::Stdout {
                self.emit_output(text, false);
            } else if io_kind == IOKind::Stderr {
                self.emit_stderr(text, false);
            } else if io_kind == IOKind::Devnull {
                // discard
            } else if io_kind == IOKind::Pipe {
                io_ref.borrow_mut().write_buffer_str(text).map_err(|e| {
                    VmError::ErrorException(format!("print: error writing to Pipe: {}", e))
                })?;
            } else if io_kind == IOKind::File {
                let handle_opt = io_ref.borrow().file_handle.clone();
                let Some(handle) = handle_opt else {
                    return Err(VmError::TypeError(
                        "print: file IO stream has no file handle".to_string(),
                    ));
                };
                handle.borrow_mut().write_str(text).map_err(|e| {
                    VmError::ErrorException(format!("print: error writing to file: {}", e))
                })?;
            } else {
                return Err(VmError::TypeError(
                    "print: cannot write to stdin".to_string(),
                ));
            }
        } else {
            self.emit_output(text, false);
        }
        Ok(())
    }

    fn execute_runtime_print_values(
        &mut self,
        values: Vec<Value>,
        newline: bool,
    ) -> Result<(), VmError> {
        let sink = if newline || values.len() > 1 {
            values
                .first()
                .and_then(|value| self.print_io_sink_value(value))
        } else {
            None
        };
        let start = usize::from(sink.is_some());
        let sink = sink.as_ref();

        // Byte-exact 2-arg print for invalid-UTF-8 carriers (Issue #8995):
        // must run BEFORE the exact print-display dispatch, which renders
        // through the lossy String pipeline.
        if !newline && start == 1 && values.len() == 2 {
            if let Some(bytes) = invalid_utf8_print_bytes(&values[1]) {
                if self.emit_print_raw_bytes_to_sink(sink, &bytes)? {
                    return Ok(());
                }
            }
        }

        if !newline
            && start == 1
            && values.len() == 2
            && self.try_start_exact_print_display(&values[0], &values[1])?
        {
            return Ok(());
        }

        for value in values.iter().skip(start) {
            // Byte-exact print for invalid-UTF-8 carriers into an IOBuffer /
            // Pipe sink (Issue #8995): `print(io, s)` must preserve the raw
            // bytes so `String(take!(io))` round-trips (the `string(...)`
            // pure-Julia path is built on this). Non-buffer sinks keep the
            // lossy text render — the host output pipeline is String-typed.
            if let Some(bytes) = invalid_utf8_print_bytes(value) {
                if self.emit_print_raw_bytes_to_sink(sink, &bytes)? {
                    continue;
                }
            }
            let rendered = self.render_print_value(value);
            self.emit_print_text_to_sink(sink, &rendered)?;
        }
        if newline {
            self.emit_print_text_to_sink(sink, "\n")?;
        }
        Ok(())
    }

    /// Write raw bytes to the print sink when it is a Buffer/Pipe IO.
    /// Returns `Ok(false)` when the sink is not byte-addressable (caller
    /// falls back to the lossy text path).
    fn emit_print_raw_bytes_to_sink(
        &mut self,
        sink: Option<&Value>,
        bytes: &[u8],
    ) -> Result<bool, VmError> {
        let resolved_sink = sink.and_then(|value| self.print_io_sink_value(value));
        if let Some(Value::IO(io_ref)) = resolved_sink.as_ref() {
            let io_kind = io_ref.borrow().kind.clone();
            if matches!(io_kind, IOKind::Buffer | IOKind::Pipe) {
                io_ref.borrow_mut().write_buffer_bytes(bytes).map_err(|e| {
                    VmError::ErrorException(format!("print: error writing to IOBuffer: {}", e))
                })?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Execute I/O builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not an I/O builtin.
    pub(super) fn execute_builtin_io(
        &mut self,
        builtin: &BuiltinId,
        _argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            // =========================================================================
            // I/O Operations
            // =========================================================================
            BuiltinId::Print => {
                let values = self.pop_builtin_values(_argc)?;
                self.execute_runtime_print_values(values, false)?;
            }
            BuiltinId::Println => {
                let values = self.pop_builtin_values(_argc)?;
                self.execute_runtime_print_values(values, true)?;
            }
            BuiltinId::EmitDisplayArtifact => {
                // _display_artifact(x) — the multimedia display-stack hook
                // (Issue #9262). Pure-Julia `display(x)` calls this so a graphical
                // host can render `x` (e.g. `display(plot(cos))` on the iOS/web
                // REPL) instead of dumping the struct text. Returns a Bool telling
                // `display` whether the value was handled graphically:
                //   * host graphical display inactive (CLI/script)      -> false
                //   * value is not a renderable artifact (e.g. a number) -> false
                //   * artifact produced and buffered in the VM sink      -> true
                // On `false`, `display` falls back to text output, matching a
                // headless Julia session. `try_value_to_artifact` is the SAME
                // structural path used for the trailing-expression render — no
                // type-name/package-name special-casing.
                let values = self.pop_builtin_values(_argc)?;
                let value = values.into_iter().next_back().unwrap_or(Value::Nothing);
                let handled = if self.graphical_display_active() {
                    match crate::plotting::try_value_to_artifact(&value, &self.struct_heap) {
                        Some(artifact) => {
                            self.push_display_artifact(artifact);
                            true
                        }
                        None => false,
                    }
                } else {
                    false
                };
                self.stack.push(Value::Bool(handled));
            }

            // =========================================================================
            // IOBuffer Operations
            // =========================================================================
            BuiltinId::IOBufferNew => {
                // IOBuffer() - create new empty IOBuffer
                self.stack.push(Value::IO(IOValue::buffer_ref()));
            }
            BuiltinId::PipeNew => {
                self.stack.push(Value::IO(IOValue::pipe_ref()));
            }
            BuiltinId::IOBufferFromString => {
                // IOBuffer(s) - create a readable buffer initialized with `s`
                // (Issue #5686). `read(io, String)` / `String(take!(io))` return it.
                let val = self.stack.pop_value()?;
                let buffer = match val {
                    Value::Str(s) => s.as_bytes().to_vec(),
                    Value::StrBytes(bytes) => bytes.to_vec(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "IOBuffer: expected a String argument, got {:?}",
                            val.value_type()
                        )))
                    }
                };
                let io = IOValue {
                    kind: IOKind::Buffer,
                    buffer,
                    buffer_pos: 0,
                    file_handle: None,
                };
                self.stack.push(Value::IO(io.into_ref()));
            }
            BuiltinId::TakeString => {
                // take!(io) - extract bytes from IOBuffer and clear it
                let val = self.stack.pop_value()?;
                match val {
                    Value::IO(io_ref) => {
                        let mut io = io_ref.borrow_mut();
                        let result = std::mem::take(&mut io.buffer);
                        let len = result.len();
                        io.buffer_pos = 0;
                        let bytes = ArrayValue::memory_first_from_u8(result, vec![len]);
                        self.stack.push(native_array_value_from_array(bytes));
                    }
                    // Non-IO receivers dispatch to the pure-Julia take!
                    // methods (`take!(c::Channel)` in base/channels.jl):
                    // closures/@async bodies compile `take!` to this builtin
                    // when the receiver type is unknown at compile time
                    // (Issue #10352). Mirrors the Length struct fallback.
                    val @ (Value::Struct(_) | Value::StructRef(_)) => {
                        let args = vec![val];
                        if let Some(func_index) =
                            self.find_best_method_index(&["take!", "Base.take!"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(Some(()));
                        }
                        let type_name = self.get_type_name(&args[0]);
                        return Err(VmError::MethodError(format!(
                            "no method matching take!({})",
                            type_name
                        )));
                    }
                    _ => return Err(VmError::TypeError("take! requires an IOBuffer".to_string())),
                }
            }
            BuiltinId::IOWrite => {
                // write(io, x) - write bytes to an IO stream, returning byte count
                let val = self.stack.pop_value()?;
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        // Issue #4741: write uses bare names for Symbols too.
                        // Issue #4761: resolve heap-allocated StructRefs so
                        // `write(io, Pair(1, 2))` emits "1 => 2" instead of
                        // the Rust debug `StructRef(heap_idx=N)` repr.
                        let resolved = crate::vm::formatting::resolve_struct_refs_for_format(
                            &val,
                            &self.struct_heap,
                        );
                        let bytes = iowrite_payload_bytes(&resolved);
                        let io_kind = io_ref.borrow().kind.clone();
                        match io_kind {
                            IOKind::Buffer | IOKind::Pipe => {
                                io_ref
                                    .borrow_mut()
                                    .write_buffer_bytes(&bytes)
                                    .map_err(|e| {
                                        VmError::ErrorException(format!(
                                            "write: error writing to IO stream: {}",
                                            e
                                        ))
                                    })?;
                            }
                            IOKind::File => {
                                let handle_opt = io_ref.borrow().file_handle.clone();
                                let Some(handle) = handle_opt else {
                                    return Err(VmError::TypeError(
                                        "write: file IO stream has no file handle".to_string(),
                                    ));
                                };
                                handle.borrow_mut().write_bytes(&bytes).map_err(|e| {
                                    VmError::ErrorException(format!(
                                        "write: error writing to file: {}",
                                        e
                                    ))
                                })?;
                            }
                            IOKind::Stdout => {
                                let s = String::from_utf8_lossy(&bytes);
                                self.emit_output(&s, false);
                            }
                            IOKind::Stderr => {
                                let s = String::from_utf8_lossy(&bytes);
                                self.emit_stderr(&s, false);
                            }
                            IOKind::Devnull => {}
                            IOKind::Stdin => {
                                return Err(VmError::TypeError(
                                    "write: cannot write to stdin".to_string(),
                                ))
                            }
                        }
                        self.stack.push(Value::I64(bytes.len() as i64));
                    }
                    _ => {
                        return Err(VmError::TypeError(
                            "write requires an IO stream as first argument".to_string(),
                        ))
                    }
                }
            }

            BuiltinId::IOPrint => {
                // print(io, args...) - print multiple args to IOBuffer (modifies in place), returns IO
                // Or print(args...) when first arg is not IO - prints to stdout
                // Stack: [arg1, arg2, ..., argN] (args pushed in order)
                // The _argc in CallBuiltin is the total number of args
                let total_args = _argc;
                if total_args == 0 {
                    // print() with no args - just return nothing
                    self.stack.push(Value::Nothing);
                    return Ok(Some(()));
                }

                // Pop the values (they're in reverse order on stack)
                let mut values = Vec::with_capacity(total_args);
                for _ in 0..total_args {
                    values.push(self.stack.pop_value()?);
                }
                // Reverse to get correct order: [arg1, arg2, ...]
                values.reverse();

                // Issue #4761: like `BuiltinId::Print` does for `print(x)`,
                // dereference any `Value::StructRef` against the heap before
                // formatting so heap-allocated structs (e.g. `Pair(1, 2)`,
                // user structs) render via `format_value_print` instead of
                // leaking the Rust debug `StructRef(heap_idx=N)` repr into
                // the IOBuffer / stdout / stderr.
                let resolved_values: Vec<Value> = values
                    .iter()
                    .map(|v| {
                        crate::vm::formatting::resolve_struct_refs_for_format(v, &self.struct_heap)
                    })
                    .collect();
                let first_sink = self.print_io_sink_value(&values[0]);

                // Issue #4761/#9460: dispatch to registered two-argument IO
                // display methods for the exact `print(io, x)` shape. Normal
                // print paths prefer `Base.print(io, ::T)` and fall back to
                // `Base.show(io, ::T)`. When generic `show(io::IO, x)` delegates
                // here to redispatch a statically-Any value, keep show-form
                // semantics by selecting only the show registry.
                if total_args == 2 && first_sink.is_some() {
                    let display_func_index = if self.current_frame_is_generic_show_fallback() {
                        self.user_show_method_for_io(&resolved_values[1], &values[0])
                    } else {
                        self.user_show_method_for_print_io(&resolved_values[1], &values[0])
                    };
                    if let Some(func_index) = display_func_index {
                        let io_for_call = values[0].clone();
                        let val_for_call = resolved_values[1].clone();
                        // display method call: pass the original sink as first
                        // param so IOContext properties remain visible.
                        let args = vec![io_for_call, val_for_call];
                        self.start_function_call(func_index, args)?;
                        return Ok(Some(()));
                    }
                }

                // Check if first value is IO-like (plain IO, or IOContext whose
                // `.io` field resolves to a plain IO sink).
                match first_sink.as_ref() {
                    Some(Value::IO(io_ref)) => {
                        let io_kind = io_ref.borrow().kind.clone();
                        // Pre-render each value before borrowing the sink. This
                        // is the runtime fallback for `Any`-typed IO handles
                        // that compile to one `IOPrint(N)`: each argument must
                        // still use the same print/show display path as the
                        // split `IOPrint(2)` lowering (Issues #4827/#7893).
                        // Invalid-UTF-8 carriers keep their raw bytes when the
                        // sink is byte-addressable (Issue #8995).
                        let rendered: Vec<Vec<u8>> = (1..resolved_values.len())
                            .map(|i| {
                                invalid_utf8_print_bytes(&resolved_values[i]).unwrap_or_else(|| {
                                    self.render_print_value(&resolved_values[i]).into_bytes()
                                })
                            })
                            .collect();

                        if io_kind == IOKind::Buffer || io_kind == IOKind::Pipe {
                            {
                                let mut io = io_ref.borrow_mut();
                                for s in &rendered {
                                    io.write_buffer_bytes(s).map_err(|e| {
                                        VmError::ErrorException(format!(
                                            "print: error writing to IOBuffer: {}",
                                            e
                                        ))
                                    })?;
                                }
                            }
                            // Return the same IORef (now mutated)
                            self.stack.push(values[0].clone());
                        } else if self.sprint_state.is_some() {
                            // Preserve the historical subset behavior that stdout/stderr
                            // writes inside sprint are captured by the active sprint buffer,
                            // but do not steal explicit IOBuffer writes from nested buffers.
                            for s in &rendered {
                                self.emit_output(&String::from_utf8_lossy(s), false);
                            }
                            // Return nothing for sprint context
                            self.stack.push(Value::Nothing);
                        } else if io_kind == IOKind::Stdout {
                            // For stdout, just print to stdout (no IOBuffer to update)
                            for s in &rendered {
                                self.emit_output(&String::from_utf8_lossy(s), false);
                            }
                            // Return nothing for stdout (like regular print)
                            self.stack.push(Value::Nothing);
                        } else if io_kind == IOKind::Stderr {
                            // Issue #3573: route stderr writes through `emit_stderr`
                            // so they reach the captured stderr buffer (forwarded
                            // by the runner to the user's actual stderr on exit).
                            for s in &rendered {
                                self.emit_stderr(&String::from_utf8_lossy(s), false);
                            }
                            self.stack.push(Value::Nothing);
                        } else if io_kind == IOKind::Devnull {
                            // /dev/null: discard all output.
                            self.stack.push(Value::Nothing);
                        } else if io_kind == IOKind::File {
                            let handle_opt = io_ref.borrow().file_handle.clone();
                            let Some(handle) = handle_opt else {
                                return Err(VmError::TypeError(
                                    "print: file IO stream has no file handle".to_string(),
                                ));
                            };
                            for s in &rendered {
                                handle.borrow_mut().write_bytes(s).map_err(|e| {
                                    VmError::ErrorException(format!(
                                        "print: error writing to file: {}",
                                        e
                                    ))
                                })?;
                            }
                            self.stack.push(Value::Nothing);
                        } else {
                            return Err(VmError::TypeError(
                                "print: cannot write to stdin".to_string(),
                            ));
                        }
                    }
                    _ => {
                        // First arg is not IO - print all values to stdout.
                        // Issue #7171/#7172: a lone non-IO value with a registered
                        // `show(io, ::T)` (e.g. `println(factor(360))`, which lowers
                        // to `IOPrint(value)` + `IOPrintlnNewline`) must dispatch to
                        // that user `show` — matching the stdout `PrintAnyNoNewline`
                        // path — instead of the generic field dump below.
                        if resolved_values.len() == 1 {
                            if let Some(func_index) =
                                self.user_show_method_for_print(&resolved_values[0])
                            {
                                let stdout = Value::IO(self.current_stdout.clone());
                                let args = vec![stdout, resolved_values[0].clone()];
                                self.start_function_call(func_index, args)?;
                                return Ok(Some(()));
                            }
                            // Issue #7893: a lone array whose struct elements have
                            // a registered `Base.show` (e.g. `println([x y; x x])`,
                            // which lowers to `IOPrint(value)` + `IOPrintlnNewline`)
                            // renders each element via that method.
                            if let Some(s) = self.render_array_via_user_show(&resolved_values[0]) {
                                self.emit_output(&s, false);
                                self.stack.push(Value::Nothing);
                                return Ok(Some(()));
                            }
                        }
                        // `resolved_values` was heap-resolved above, so each
                        // element wraps cleanly with `Resolved::trivial` (Issue #8642).
                        for val in &resolved_values {
                            let s =
                                crate::vm::formatting::format_value_print(&Resolved::trivial(val));
                            self.emit_output(&s, false);
                        }
                        self.stack.push(Value::Nothing);
                    }
                }
            }

            BuiltinId::RedirectStdout | BuiltinId::RedirectStderr => {
                let kind = if matches!(builtin, BuiltinId::RedirectStdout) {
                    RedirectStreamKind::Stdout
                } else {
                    RedirectStreamKind::Stderr
                };
                self.execute_redirect_stdio(kind, _argc)?;
            }

            BuiltinId::Displaysize => {
                // displaysize() - return terminal size as (rows, cols)
                // Returns default values since SubsetJuliaVM typically runs
                // in environments without a terminal (iOS, WASM, etc.)
                let rows = Value::I64(24);
                let cols = Value::I64(80);
                self.stack.push(Value::Tuple(TupleValue {
                    elements: vec![rows, cols],
                }));
            }

            // =========================================================================
            // Source File Loading (no-ops)
            // =========================================================================
            BuiltinId::IncludeDependency => {
                // include_dependency(path) - track file dependency for precompilation
                // Since precompilation is not yet implemented, this is a no-op
                // that accepts a path argument and returns nothing
                let _path = self.stack.pop_value()?;
                self.stack.push(Value::Nothing);
            }

            BuiltinId::Precompile => {
                // __precompile__(flag) - control module precompilation
                // Since precompilation is not yet implemented, this is a no-op
                // that accepts a boolean argument and returns nothing
                let _flag = self.stack.pop_value()?;
                self.stack.push(Value::Nothing);
            }

            // =========================================================================
            // Path/Filesystem Operations
            // Note: dirname, basename, joinpath, splitext, splitdir, isabspath, isdirpath
            // are now Pure Julia (base/path.jl) — Issue #2637
            // =========================================================================
            BuiltinId::Normpath => {
                // normpath(path) - normalize path (remove . and ..)
                let path_val = self.stack.pop_value()?;
                let path_str = if let Value::Str(s) = path_val {
                    s
                } else {
                    return Err(VmError::TypeError(format!(
                        "normpath requires a String, got {:?}",
                        path_val
                    )));
                };

                use std::path::{Component, Path, PathBuf};
                let path = Path::new(&*path_str);
                let mut normalized = PathBuf::new();
                for component in path.components() {
                    match component {
                        Component::Prefix(p) => normalized.push(p.as_os_str()),
                        Component::RootDir => normalized.push("/"),
                        Component::CurDir => {} // Skip "."
                        Component::ParentDir => {
                            // Pop the last component if possible
                            if !normalized.pop() {
                                normalized.push("..");
                            }
                        }
                        Component::Normal(c) => normalized.push(c),
                    }
                }
                if normalized.as_os_str().is_empty() {
                    normalized.push(".");
                }

                self.stack
                    .push(Value::str_new(normalized.to_string_lossy().to_string()));
            }

            BuiltinId::Abspath => {
                // abspath(path) - convert to absolute path
                let path_val = self.stack.pop_value()?;
                let path_str = if let Value::Str(s) = path_val {
                    s
                } else {
                    return Err(VmError::TypeError(format!(
                        "abspath requires a String, got {:?}",
                        path_val
                    )));
                };

                use std::path::Path;
                let path = Path::new(&*path_str);
                let abs_path = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    std::env::current_dir().unwrap_or_default().join(path)
                };

                self.stack
                    .push(Value::str_new(abs_path.to_string_lossy().to_string()));
            }

            BuiltinId::Homedir => {
                // homedir() - get home directory
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .unwrap_or_else(|_| "/".to_string());
                self.stack.push(Value::str_new(home));
            }

            // =========================================================================
            // Time Operations
            // =========================================================================
            BuiltinId::Sleep => {
                let secs = self.pop_f64_or_i64()?;
                std::thread::sleep(std::time::Duration::from_secs_f64(secs));
                self.stack.push(Value::Nothing);
            }
            BuiltinId::TimeNs => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                self.stack.push(Value::I64(now.as_nanos() as i64));
            }

            // =========================================================================
            // File I/O Operations (read-only)
            // =========================================================================
            BuiltinId::ReadFile => {
                // read(filename, String) - read entire file contents as String
                // Stack: [filename, String_type] -> read file -> push string
                // Pop the type argument (String) - we ignore it since we always return String
                let _type_arg = self.stack.pop_value()?;
                let filename = self.stack.pop_value()?;
                let path = match filename {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "read: expected String for filename, got {:?}",
                            filename
                        )))
                    }
                };
                {
                    let contents = read_text_file(&path, "read")?;
                    self.stack.push(Value::str_new(contents))
                }
            }
            BuiltinId::ReadLines | BuiltinId::Eachline => {
                // readlines(filename) / eachline(filename) - read all lines as Vector{String}
                let fn_name = if matches!(builtin, BuiltinId::Eachline) {
                    "eachline"
                } else {
                    "readlines"
                };
                let filename = self.stack.pop_value()?;
                let path = match filename {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "{}: expected String for filename, got {:?}",
                            fn_name, filename
                        )))
                    }
                };
                {
                    let contents = read_text_file(&path, fn_name)?;
                    let lines: Vec<Value> = contents
                        .lines()
                        .map(|line| Value::str_new(line.to_string()))
                        .collect();
                    let arr = ArrayValue::any_vector(lines);
                    self.push_array_value_as_wrapper(arr)?;
                }
            }
            BuiltinId::Readline => {
                // readline(filename) - read first line from file
                let filename = self.stack.pop_value()?;
                let path = match filename {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "readline: expected String for filename, got {:?}",
                            filename
                        )))
                    }
                };
                {
                    let contents = read_text_file(&path, "readline")?;
                    let line = contents.lines().next().unwrap_or_default();
                    self.stack.push(Value::str_new(line.to_string()));
                }
            }
            BuiltinId::Countlines => {
                // countlines(filename) - count lines in file
                let filename = self.stack.pop_value()?;
                let path = match filename {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "countlines: expected String for filename, got {:?}",
                            filename
                        )))
                    }
                };
                {
                    let contents = read_text_file(&path, "countlines")?;
                    self.stack.push(Value::I64(contents.lines().count() as i64));
                }
            }
            BuiltinId::Isfile => {
                // isfile(path) - check if path is a regular file
                let path_val = self.stack.pop_value()?;
                let path = match path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "isfile: expected String, got {:?}",
                            path_val
                        )))
                    }
                };
                let result = std::path::Path::new(&path).is_file()
                    || crate::julia::packages::get_package_file(&path).is_some();
                self.stack.push(Value::Bool(result));
            }
            BuiltinId::Isdir => {
                // isdir(path) - check if path is a directory
                let path_val = self.stack.pop_value()?;
                let path = match path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "isdir: expected String, got {:?}",
                            path_val
                        )))
                    }
                };
                let result = std::path::Path::new(&path).is_dir();
                self.stack.push(Value::Bool(result));
            }
            BuiltinId::Ispath => {
                // ispath(path) - check if path exists (file or directory)
                let path_val = self.stack.pop_value()?;
                let path = match path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "ispath: expected String, got {:?}",
                            path_val
                        )))
                    }
                };
                let result = std::path::Path::new(&path).exists()
                    || crate::julia::packages::get_package_file(&path).is_some();
                self.stack.push(Value::Bool(result));
            }
            BuiltinId::Filesize => {
                // filesize(path) - get file size in bytes
                let path_val = self.stack.pop_value()?;
                let path = match path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "filesize: expected String, got {:?}",
                            path_val
                        )))
                    }
                };
                match std::fs::metadata(&path) {
                    Ok(meta) => {
                        self.stack.push(Value::I64(meta.len() as i64));
                    }
                    Err(e) => {
                        if let Some(contents) = crate::julia::packages::get_package_file(&path) {
                            self.stack.push(Value::I64(contents.len() as i64));
                        } else {
                            return Err(VmError::ErrorException(format!(
                                "filesize: failed to get metadata for '{}': {}",
                                path, e
                            )));
                        }
                    }
                }
            }

            BuiltinId::Pwd => {
                // pwd() - get current working directory
                match std::env::current_dir() {
                    Ok(path) => {
                        let path_str = path.to_string_lossy().to_string();
                        self.stack.push(Value::str_new(path_str));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "pwd: failed to get current directory: {}",
                            e
                        )))
                    }
                }
            }

            BuiltinId::Readdir => {
                // readdir(path) - list directory contents as Vector{String}
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "readdir: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                match std::fs::read_dir(&path) {
                    Ok(entries) => {
                        let mut names: Vec<Value> = Vec::new();
                        for entry in entries {
                            match entry {
                                Ok(e) => {
                                    let name = e.file_name().to_string_lossy().to_string();
                                    names.push(Value::str_new(name));
                                }
                                Err(e) => {
                                    return Err(VmError::ErrorException(format!(
                                        "readdir: error reading entry: {}",
                                        e
                                    )))
                                }
                            }
                        }
                        // Sort the names alphabetically (like Julia does)
                        names.sort_by(|a, b| {
                            if let (Value::Str(sa), Value::Str(sb)) = (a, b) {
                                sa.cmp(sb)
                            } else {
                                std::cmp::Ordering::Equal
                            }
                        });
                        let arr = ArrayValue::any_vector(names);
                        self.push_array_value_as_wrapper(arr)?;
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "readdir: failed to read directory '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Mkdir => {
                // mkdir(path) - create directory
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "mkdir: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                match std::fs::create_dir(&path) {
                    Ok(()) => {
                        self.stack.push(Value::str_new(path));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "mkdir: failed to create directory '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Mkpath => {
                // mkpath(path) - create directory and all parents
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "mkpath: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                match std::fs::create_dir_all(&path) {
                    Ok(()) => {
                        self.stack.push(Value::str_new(path));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "mkpath: failed to create path '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Rm => {
                // rm(path) - remove file or empty directory
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "rm: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                // Check if it's a file or directory
                let metadata = std::fs::metadata(&path);
                match metadata {
                    Ok(m) => {
                        if m.is_dir() {
                            match std::fs::remove_dir(&path) {
                                Ok(()) => {
                                    self.stack.push(Value::Nothing);
                                }
                                Err(e) => {
                                    return Err(VmError::ErrorException(format!(
                                        "rm: failed to remove directory '{}': {}",
                                        path, e
                                    )))
                                }
                            }
                        } else {
                            match std::fs::remove_file(&path) {
                                Ok(()) => {
                                    self.stack.push(Value::Nothing);
                                }
                                Err(e) => {
                                    return Err(VmError::ErrorException(format!(
                                        "rm: failed to remove file '{}': {}",
                                        path, e
                                    )))
                                }
                            }
                        }
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "rm: path '{}' not found: {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Tempdir => {
                // tempdir() - get system temp directory
                let temp_dir = std::env::temp_dir();
                let path_str = temp_dir.to_string_lossy().to_string();
                self.stack.push(Value::str_new(path_str));
            }

            BuiltinId::Tempname => {
                // tempname() - generate unique temp filename
                let temp_dir = std::env::temp_dir();
                // Generate a random suffix using timestamp and a counter
                use std::time::{SystemTime, UNIX_EPOCH};
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                // Use a simple random-like value from timestamp
                let random_part = format!("{:x}", timestamp);
                let filename = format!(
                    "jl_{}",
                    &random_part[random_part.len().saturating_sub(12)..]
                );
                let path = temp_dir.join(&filename);
                let path_str = path.to_string_lossy().to_string();
                self.stack.push(Value::str_new(path_str));
            }

            BuiltinId::Touch => {
                // touch(path) - create empty file or update mtime
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "touch: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                // If file exists, open with append to update mtime
                // If not exists, create empty file
                use std::fs::OpenOptions;
                match OpenOptions::new()
                    .create(true)
                    .append(true) // append mode updates mtime on open
                    .open(&path)
                {
                    Ok(_file) => {
                        // File is created or mtime updated by opening
                        self.stack.push(Value::str_new(path));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "touch: failed to touch file '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Cd => {
                // cd(path) - change current directory
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "cd: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                match std::env::set_current_dir(&path) {
                    Ok(()) => {
                        // Return the new working directory
                        match std::env::current_dir() {
                            Ok(cwd) => {
                                self.stack
                                    .push(Value::str_new(cwd.to_string_lossy().to_string()));
                            }
                            Err(_) => {
                                self.stack.push(Value::str_new(path));
                            }
                        }
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "cd: failed to change directory to '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Islink => {
                // islink(path) - check if path is a symbolic link
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "islink: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                // Use symlink_metadata to get info about the link itself, not the target
                let is_link = std::fs::symlink_metadata(&path)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false);
                self.stack.push(Value::Bool(is_link));
            }

            BuiltinId::Cp => {
                // cp(src, dst) - copy file
                let dst_val = self.stack.pop_value()?;
                let src_val = self.stack.pop_value()?;
                let src = match &src_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "cp: source path must be a string, got {:?}",
                            src_val
                        )))
                    }
                };
                let dst = match &dst_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "cp: destination path must be a string, got {:?}",
                            dst_val
                        )))
                    }
                };
                match std::fs::copy(&src, &dst) {
                    Ok(_bytes_copied) => {
                        self.stack.push(Value::str_new(dst));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "cp: failed to copy '{}' to '{}': {}",
                            src, dst, e
                        )))
                    }
                }
            }

            BuiltinId::Mv => {
                // mv(src, dst) - move/rename file
                let dst_val = self.stack.pop_value()?;
                let src_val = self.stack.pop_value()?;
                let src = match &src_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "mv: source path must be a string, got {:?}",
                            src_val
                        )))
                    }
                };
                let dst = match &dst_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "mv: destination path must be a string, got {:?}",
                            dst_val
                        )))
                    }
                };
                match std::fs::rename(&src, &dst) {
                    Ok(()) => {
                        self.stack.push(Value::str_new(dst));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "mv: failed to move '{}' to '{}': {}",
                            src, dst, e
                        )))
                    }
                }
            }

            BuiltinId::Mtime => {
                // mtime(path) - get modification time as Unix timestamp (Float64)
                let path_val = self.stack.pop_value()?;
                let path = match &path_val {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "mtime: path must be a string, got {:?}",
                            path_val
                        )))
                    }
                };
                use std::time::UNIX_EPOCH;
                match std::fs::metadata(&path) {
                    Ok(metadata) => {
                        match metadata.modified() {
                            Ok(modified_time) => {
                                let duration =
                                    modified_time.duration_since(UNIX_EPOCH).unwrap_or_default();
                                // Julia returns seconds as Float64
                                let secs = duration.as_secs_f64();
                                self.stack.push(Value::F64(secs));
                            }
                            Err(e) => {
                                return Err(VmError::ErrorException(format!(
                                    "mtime: failed to get modification time for '{}': {}",
                                    path, e
                                )))
                            }
                        }
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "mtime: failed to get metadata for '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            // =========================================================================
            // File Handle Operations
            // =========================================================================
            BuiltinId::Open => {
                // open(filename) - open for reading
                // open(filename, mode) - open with mode
                let (path, mode) = if _argc == 2 {
                    let mode_val = self.stack.pop_value()?;
                    let path_val = self.stack.pop_value()?;
                    let path = match path_val {
                        Value::Str(s) => s.to_string(),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "open: expected String for filename, got {:?}",
                                path_val
                            )))
                        }
                    };
                    let mode = match mode_val {
                        Value::Str(s) => s.to_string(),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "open: expected String for mode, got {:?}",
                                mode_val
                            )))
                        }
                    };
                    (path, mode)
                } else {
                    let path_val = self.stack.pop_value()?;
                    let path = match path_val {
                        Value::Str(s) => s.to_string(),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "open: expected String for filename, got {:?}",
                                path_val
                            )))
                        }
                    };
                    (path, "r".to_string())
                };

                // Parse mode string (like Julia's fopen)
                let (readable, writable, create, truncate, append) = match mode.as_str() {
                    "r" => (true, false, false, false, false),
                    "r+" => (true, true, false, false, false),
                    "w" => (false, true, true, true, false),
                    "w+" => (true, true, true, true, false),
                    "a" => (false, true, true, false, true),
                    "a+" => (true, true, true, false, true),
                    _ => {
                        return Err(VmError::ErrorException(format!(
                            "open: invalid mode '{}'. Valid modes are: r, r+, w, w+, a, a+",
                            mode
                        )))
                    }
                };

                use std::fs::OpenOptions;
                let file_result = OpenOptions::new()
                    .read(readable)
                    .write(writable)
                    .create(create)
                    .truncate(truncate)
                    .append(append)
                    .open(&path);

                match file_result {
                    Ok(file) => {
                        let io_val = IOValue::file_from(file, path, readable, writable);
                        self.stack.push(Value::IO(io_val.into_ref()));
                    }
                    Err(e) => {
                        return Err(VmError::ErrorException(format!(
                            "open: failed to open file '{}': {}",
                            path, e
                        )))
                    }
                }
            }

            BuiltinId::Close => {
                // close(io) - close IO stream
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        let io = io_ref.borrow();
                        if let Some(ref handle) = io.file_handle {
                            handle.borrow_mut().close();
                        }
                        // For other IO types, close is a no-op
                        self.stack.push(Value::Nothing);
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "close: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            BuiltinId::Eof => {
                // eof(io) - check if at end of file
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        let io = io_ref.borrow();
                        let at_eof = if let Some(ref handle) = io.file_handle {
                            match handle.borrow_mut().eof() {
                                Ok(eof) => eof,
                                Err(e) => {
                                    return Err(VmError::ErrorException(format!(
                                        "eof: error checking EOF: {}",
                                        e
                                    )))
                                }
                            }
                        } else {
                            match io.kind {
                                IOKind::Buffer => io.buffer_eof(),
                                _ => false, // stdout/stderr/stdin are never at EOF
                            }
                        };
                        self.stack.push(Value::Bool(at_eof));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "eof: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            BuiltinId::Seek => {
                // seek(io, pos) - set the file cursor to an absolute byte position
                let pos = self.stack.pop_i64()?;
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        let io_kind = io_ref.borrow().kind.clone();
                        match io_kind {
                            IOKind::Buffer => {
                                io_ref.borrow_mut().seek_buffer_start(pos);
                            }
                            IOKind::File => {
                                if pos < 0 {
                                    return Err(VmError::ErrorException(format!(
                                        "seek: position must be non-negative, got {}",
                                        pos
                                    )));
                                }
                                let pos = u64::try_from(pos).map_err(|_| {
                                    VmError::ErrorException(format!(
                                        "seek: position is out of range: {}",
                                        pos
                                    ))
                                })?;
                                let handle_opt = io_ref.borrow().file_handle.clone();
                                let Some(handle) = handle_opt else {
                                    return Err(VmError::TypeError(
                                        "seek: file IO stream has no file handle".to_string(),
                                    ));
                                };
                                handle.borrow_mut().seek_start(pos).map_err(|e| {
                                    VmError::ErrorException(format!(
                                        "seek: error seeking file: {}",
                                        e
                                    ))
                                })?;
                            }
                            _ => {
                                return Err(VmError::TypeError(
                                    "seek: expected seekable IO stream".to_string(),
                                ))
                            }
                        }
                        self.stack.push(Value::IO(io_ref));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "seek: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            BuiltinId::Position => {
                // position(io) - get the current file cursor byte position
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        let io_kind = io_ref.borrow().kind.clone();
                        let pos = match io_kind {
                            IOKind::Buffer => io_ref.borrow().buffer_position() as i64,
                            IOKind::File => {
                                let handle_opt = io_ref.borrow().file_handle.clone();
                                let Some(handle) = handle_opt else {
                                    return Err(VmError::TypeError(
                                        "position: file IO stream has no file handle".to_string(),
                                    ));
                                };
                                let pos = handle.borrow_mut().position().map_err(|e| {
                                    VmError::ErrorException(format!(
                                        "position: error reading file position: {}",
                                        e
                                    ))
                                })?;
                                i64::try_from(pos).map_err(|_| {
                                    VmError::ErrorException(format!(
                                        "position: file position {} does not fit in Int64",
                                        pos
                                    ))
                                })?
                            }
                            _ => {
                                return Err(VmError::TypeError(
                                    "position: expected seekable IO stream".to_string(),
                                ))
                            }
                        };
                        self.stack.push(Value::I64(pos));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "position: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            BuiltinId::Skip => {
                // skip(io, n) - move the file cursor relative to current position
                let offset = self.stack.pop_i64()?;
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        let io_kind = io_ref.borrow().kind.clone();
                        match io_kind {
                            IOKind::Buffer => {
                                io_ref.borrow_mut().skip_buffer(offset);
                            }
                            IOKind::File => {
                                let handle_opt = io_ref.borrow().file_handle.clone();
                                let Some(handle) = handle_opt else {
                                    return Err(VmError::TypeError(
                                        "skip: file IO stream has no file handle".to_string(),
                                    ));
                                };
                                handle.borrow_mut().skip(offset).map_err(|e| {
                                    VmError::ErrorException(format!(
                                        "skip: error seeking file: {}",
                                        e
                                    ))
                                })?;
                            }
                            _ => {
                                return Err(VmError::TypeError(
                                    "skip: expected seekable IO stream".to_string(),
                                ))
                            }
                        }
                        self.stack.push(Value::IO(io_ref));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "skip: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            BuiltinId::Flush => {
                // flush(io) - flush pending writes; non-file streams are already synchronous
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        let handle_opt = io_ref.borrow().file_handle.clone();
                        if let Some(handle) = handle_opt {
                            handle.borrow_mut().flush().map_err(|e| {
                                VmError::ErrorException(format!(
                                    "flush: error flushing file: {}",
                                    e
                                ))
                            })?;
                        }
                        self.stack.push(Value::Nothing);
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "flush: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            BuiltinId::Isopen => {
                // isopen(io) - check if IO stream is open
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        let io = io_ref.borrow();
                        let is_open = io.is_open();
                        self.stack.push(Value::Bool(is_open));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "isopen: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            BuiltinId::ReadCharIo => {
                // read(io, Char) - read one character from an IO stream
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        let io_kind = io_ref.borrow().kind.clone();
                        let ch = match io_kind {
                            IOKind::Buffer => {
                                io_ref.borrow_mut().read_buffer_char().map_err(|e| {
                                    VmError::ErrorException(format!(
                                        "read: error reading Char from IOBuffer: {}",
                                        e
                                    ))
                                })?
                            }
                            IOKind::File => {
                                let handle_opt = io_ref.borrow().file_handle.clone();
                                let Some(handle) = handle_opt else {
                                    return Err(VmError::TypeError(
                                        "read: file IO stream has no file handle".to_string(),
                                    ));
                                };
                                let ch = handle.borrow_mut().read_char().map_err(|e| {
                                    VmError::ErrorException(format!(
                                        "read: error reading Char from file: {}",
                                        e
                                    ))
                                })?;
                                ch
                            }
                            _ => {
                                return Err(VmError::TypeError(
                                    "read: expected readable IO stream for read(io, Char)"
                                        .to_string(),
                                ))
                            }
                        };
                        self.stack.push(Value::Char(ch));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "read: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            BuiltinId::ReadlineIo => {
                // readline(io) - read line from IO stream
                let io_val = self.stack.pop_value()?;
                match io_val {
                    Value::IO(io_ref) => {
                        // Clone the file handle to avoid borrow issues
                        let handle_opt = {
                            let io = io_ref.borrow();
                            io.file_handle.clone()
                        };

                        if let Some(handle) = handle_opt {
                            match handle.borrow_mut().readline() {
                                Ok(Some(line)) => {
                                    self.stack.push(Value::str_new(line));
                                }
                                Ok(None) => {
                                    // EOF - return empty string
                                    self.stack.push(Value::str_new(String::new()));
                                }
                                Err(e) => {
                                    return Err(VmError::ErrorException(format!(
                                        "readline: error reading line: {}",
                                        e
                                    )))
                                }
                            }
                        } else {
                            return Err(VmError::TypeError(
                                "readline: IO stream is not a file".to_string(),
                            ));
                        }
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "readline: expected IO stream, got {:?}",
                            io_val
                        )))
                    }
                }
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
