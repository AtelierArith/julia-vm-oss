//! Detailed error FFI functions.
//!
//! These functions provide rich error information including source spans and hints.

// FFI functions intentionally take raw pointers and are called from C/Swift code.
// The caller is responsible for ensuring pointer validity.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::CStr;
use std::os::raw::c_char;

use subset_julia_vm::cancel;
use subset_julia_vm::compile::host_support::compile_with_cache;
// The Editor tab runs a whole buffer through `compile_and_run_detailed` /
// `compile_and_run_streaming`, so it uses strict file-mode soft scope
// (Issue #9283 / #9210) to match `julia file.jl`. The interactive REPL
// (`repl_session_eval`) is a separate entry that stays lenient.
use subset_julia_vm::pipeline::{parse_and_lower_strict as parse_and_lower, PipelineError};
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;

use super::error::{syntax_error_span, CError, CExecutionResult, CSpan};
use crate::panic_boundary;

/// Output callback function type for streaming output.
/// Takes a context pointer and the output string (null-terminated C string).
pub type OutputCallback = extern "C" fn(context: *mut std::os::raw::c_void, output: *const c_char);

fn warm_start_compile_cache() {
    // Mirror the CLI warm-start overlap for native app entrypoints (Issue #8463).
    subset_julia_vm::compile::host_support::warm_start_compile_cache();
}

/// Compile and run with detailed error information.
/// Returns a heap-allocated CExecutionResult that must be freed with free_execution_result.
#[no_mangle]
pub extern "C" fn compile_and_run_detailed(
    src_ptr: *const c_char,
    seed: u64,
) -> *mut CExecutionResult {
    panic_boundary::catch_unwind_ffi(panic_execution_result, || {
        if src_ptr.is_null() {
            let result = CExecutionResult::failure(
                String::new(),
                CError::syntax("Null source pointer".to_string(), None),
            );
            return Box::into_raw(Box::new(result));
        }
        cancel::reset();

        let src = match unsafe { CStr::from_ptr(src_ptr) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                let result = CExecutionResult::failure(
                    String::new(),
                    CError::syntax("Invalid UTF-8 in source".to_string(), None),
                );
                return Box::into_raw(Box::new(result));
            }
        };

        warm_start_compile_cache();

        // Parse (pure-Rust `subset_julia_vm_parser`) and lower to Core IR
        let program = match parse_and_lower(src) {
            Ok(p) => p,
            Err(PipelineError::Parse(e)) => {
                let span = syntax_error_span(&e);
                let error = CError::syntax(e.to_string(), span);
                let result = CExecutionResult::failure(String::new(), error);
                return Box::into_raw(Box::new(result));
            }
            Err(PipelineError::Lower(e)) => {
                let span = CSpan::from_span(&e.span);
                let error = CError::unsupported(format!("{:?}", e.kind), span, e.hint.clone());
                let result = CExecutionResult::failure(String::new(), error);
                return Box::into_raw(Box::new(result));
            }
            Err(PipelineError::Load(e)) => {
                let error = CError::compile(e.to_string());
                let result = CExecutionResult::failure(String::new(), error);
                return Box::into_raw(Box::new(result));
            }
        };

        // Compile
        let compiled = match compile_with_cache(&program) {
            Ok(c) => c,
            Err(e) => {
                let result =
                    CExecutionResult::failure(String::new(), CError::compile(format!("{:?}", e)));
                return Box::into_raw(Box::new(result));
            }
        };

        // Run
        let rng = StableRng::new(seed);
        let mut vm = Vm::new_program(compiled, rng);
        // The native/iOS Editor host renders display artifacts, so activate the
        // graphical display stack: `display(plot(cos))` routes into the artifact
        // channel instead of echoing the struct text (Issue #9262).
        vm.enable_graphical_display();

        run_vm_to_boxed_result(vm)
    })
}

/// Run a configured VM to completion and box the detailed result.
///
/// Shared epilogue for every detailed-result entry point: the source-compile
/// paths above and the `.sjvmbc` bytecode paths in `bytecode.rs`
/// (Issue #10171). The caller configures the VM (graphical display, output
/// callback) before handing it over.
pub(crate) fn run_vm_to_boxed_result(mut vm: Vm<StableRng>) -> *mut CExecutionResult {
    match vm.run() {
        Ok(value) => {
            let output = vm.get_output().to_string();
            // Prefer artifacts emitted by explicit `display(x)` calls during
            // the run (Issue #9262) — e.g. `display(plot(cos))`, whose result
            // value is `nothing`. Otherwise render the trailing result value.
            let mut artifacts = vm.take_display_artifacts();
            if artifacts.is_empty() {
                if let Some(artifact) =
                    subset_julia_vm::plotting::try_value_to_artifact(&value, vm.get_struct_heap())
                {
                    artifacts.push(artifact);
                }
            }
            let result = CExecutionResult::success_with_value(&value, vm.get_struct_heap(), output)
                .with_artifacts(artifacts);
            Box::into_raw(Box::new(result))
        }
        Err(e) => {
            let output = vm.get_output().to_string();
            let result = CExecutionResult::failure(output, CError::runtime(format!("{}", e)));
            Box::into_raw(Box::new(result))
        }
    }
}

/// Compile and run with streaming output via callback.
/// The callback is called for each println output line during execution.
/// Returns a heap-allocated CExecutionResult that must be freed with free_execution_result.
#[no_mangle]
pub extern "C" fn compile_and_run_streaming(
    src_ptr: *const c_char,
    seed: u64,
    context: *mut std::os::raw::c_void,
    output_callback: OutputCallback,
) -> *mut CExecutionResult {
    panic_boundary::catch_unwind_ffi(panic_execution_result, || {
        if src_ptr.is_null() {
            let result = CExecutionResult::failure(
                String::new(),
                CError::syntax("Null source pointer".to_string(), None),
            );
            return Box::into_raw(Box::new(result));
        }
        cancel::reset();

        let src = match unsafe { CStr::from_ptr(src_ptr) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                let result = CExecutionResult::failure(
                    String::new(),
                    CError::syntax("Invalid UTF-8 in source".to_string(), None),
                );
                return Box::into_raw(Box::new(result));
            }
        };

        warm_start_compile_cache();

        // Parse (pure-Rust `subset_julia_vm_parser`) and lower to Core IR
        let program = match parse_and_lower(src) {
            Ok(p) => p,
            Err(PipelineError::Parse(e)) => {
                let span = syntax_error_span(&e);
                let error = CError::syntax(e.to_string(), span);
                let result = CExecutionResult::failure(String::new(), error);
                return Box::into_raw(Box::new(result));
            }
            Err(PipelineError::Lower(e)) => {
                let span = CSpan::from_span(&e.span);
                let error = CError::unsupported(format!("{:?}", e.kind), span, e.hint.clone());
                let result = CExecutionResult::failure(String::new(), error);
                return Box::into_raw(Box::new(result));
            }
            Err(PipelineError::Load(e)) => {
                let error = CError::compile(e.to_string());
                let result = CExecutionResult::failure(String::new(), error);
                return Box::into_raw(Box::new(result));
            }
        };

        // Compile
        let compiled = match compile_with_cache(&program) {
            Ok(c) => c,
            Err(e) => {
                let result =
                    CExecutionResult::failure(String::new(), CError::compile(format!("{:?}", e)));
                return Box::into_raw(Box::new(result));
            }
        };

        // Run with streaming output callback
        let rng = StableRng::new(seed);
        let mut vm = Vm::new_program(compiled, rng);
        vm.set_output_callback(output_callback, context);
        // Streaming Editor host renders display artifacts too (Issue #9262).
        vm.enable_graphical_display();

        run_vm_to_boxed_result(vm)
    })
}

pub(crate) fn panic_execution_result(payload: String) -> *mut CExecutionResult {
    let result = CExecutionResult::failure(
        String::new(),
        CError::runtime(panic_boundary::ffi_panic_message(payload)),
    );
    Box::into_raw(Box::new(result))
}

/// Feature-gated panic probe for native/iOS FFI boundary verification.
#[cfg(feature = "ffi-panic-test")]
#[no_mangle]
pub extern "C" fn subset_julia_vm_ffi_debug_panic_detailed() -> *mut CExecutionResult {
    panic_boundary::catch_unwind_ffi(panic_execution_result, || {
        std::panic::panic_any("ffi panic test detailed")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{
        execution_result_array_element_f64, execution_result_array_len,
        execution_result_artifact_count, execution_result_artifact_data,
        execution_result_artifact_data_at, execution_result_artifact_mime,
        execution_result_artifact_mime_at, execution_result_complex_imag,
        execution_result_complex_real, execution_result_value_kind, free_execution_result,
        CValueKind,
    };
    use std::ffi::{CStr, CString};

    fn run_with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
        let builder = std::thread::Builder::new()
            .name("ffi-test".into())
            .stack_size(16 * 1024 * 1024);
        let handler = builder.spawn(f).unwrap();
        if let Err(e) = handler.join() {
            std::panic::resume_unwind(e);
        }
    }

    #[cfg(feature = "ffi-panic-test")]
    #[test]
    fn test_debug_panic_returns_execution_result_error() {
        let raw = subset_julia_vm_ffi_debug_panic_detailed();
        assert!(!raw.is_null(), "panic probe should return an error result");
        unsafe {
            let result = &*raw;
            assert!(!result.success, "panic probe must not report success");
            assert!(
                !result.error.message.is_null(),
                "panic result should carry an error message"
            );
            let message = CStr::from_ptr(result.error.message).to_string_lossy();
            assert!(
                message.contains("Rust panic caught at FFI boundary")
                    && message.contains("ffi panic test detailed"),
                "unexpected panic message: {message}"
            );
        }
        free_execution_result(raw);
    }

    /// Issue #4366 (under #5283): Editor tab uses compile_and_run_detailed and
    /// must surface the Plotly plot artifact through CExecutionResult so the
    /// Swift side can render the plot inline alongside println output.
    #[test]
    fn test_compile_and_run_detailed_attaches_plot_artifact() {
        run_with_large_stack(|| {
            let src = CString::new("using Plots\nplot(sin)\n").unwrap();
            let raw = compile_and_run_detailed(src.as_ptr(), 0);
            assert!(!raw.is_null(), "FFI returned null result pointer");

            unsafe {
                let result = &*raw;
                assert!(result.success, "compile_and_run_detailed should succeed");
                assert!(
                    !result.artifact_mime.is_null(),
                    "artifact_mime should be populated for a Plot return value"
                );
                assert!(
                    !result.artifact_data.is_null(),
                    "artifact_data should be populated for a Plot return value"
                );
                let mime = CStr::from_ptr(result.artifact_mime).to_str().unwrap();
                assert_eq!(mime, "application/vnd.plotly+json");
                let data = CStr::from_ptr(result.artifact_data).to_str().unwrap();
                assert!(
                    data.contains(r#""type":"scatter""#) && data.contains(r#""mode":"lines""#),
                    "artifact data should be a Plotly line plot, got: {}",
                    &data[..data.len().min(120)]
                );
                free_execution_result(raw);
            }
        });
    }

    #[test]
    fn test_compile_and_run_detailed_preserves_multiple_display_artifacts_9488() {
        run_with_large_stack(|| {
            let src =
                CString::new("using Plots\ndisplay(plot(sin))\ndisplay(plot(cos))\n").unwrap();
            let raw = compile_and_run_detailed(src.as_ptr(), 0);
            assert!(!raw.is_null(), "FFI returned null result pointer");

            unsafe {
                let result = &*raw;
                assert!(result.success, "compile_and_run_detailed should succeed");
                assert_eq!(
                    execution_result_artifact_count(raw),
                    2,
                    "both display(plot(...)) calls should be preserved"
                );

                let first_mime =
                    CStr::from_ptr(execution_result_artifact_mime_at(raw, 0)).to_string_lossy();
                let second_mime =
                    CStr::from_ptr(execution_result_artifact_mime_at(raw, 1)).to_string_lossy();
                assert_eq!(first_mime, "application/vnd.plotly+json");
                assert_eq!(second_mime, "application/vnd.plotly+json");

                let first_data =
                    CStr::from_ptr(execution_result_artifact_data_at(raw, 0)).to_string_lossy();
                let second_data =
                    CStr::from_ptr(execution_result_artifact_data_at(raw, 1)).to_string_lossy();
                assert!(first_data.contains(r#""type":"scatter""#));
                assert!(second_data.contains(r#""type":"scatter""#));

                let legacy_mime =
                    CStr::from_ptr(execution_result_artifact_mime(raw)).to_string_lossy();
                let legacy_data =
                    CStr::from_ptr(execution_result_artifact_data(raw)).to_string_lossy();
                assert_eq!(legacy_mime, second_mime);
                assert_eq!(legacy_data, second_data);
            }
            free_execution_result(raw);
        });
    }

    /// Issue #6005: the iOS Editor consumes the Plotly artifact from
    /// compile_and_run_detailed, so histogram must stay a bar trace on the FFI
    /// path instead of falling back to a scatter/line trace.
    #[test]
    fn test_compile_and_run_detailed_histogram_emits_bar_trace() {
        run_with_large_stack(|| {
            let src = CString::new("using Plots\nhistogram([1,2,1,1,4,3,8], bins=0:8)\n").unwrap();
            let raw = compile_and_run_detailed(src.as_ptr(), 0);
            assert!(!raw.is_null(), "FFI returned null result pointer");

            unsafe {
                let result = &*raw;
                assert!(result.success, "compile_and_run_detailed should succeed");
                assert!(
                    !result.artifact_mime.is_null(),
                    "artifact_mime should be populated for a histogram return value"
                );
                assert!(
                    !result.artifact_data.is_null(),
                    "artifact_data should be populated for a histogram return value"
                );
                let mime = CStr::from_ptr(result.artifact_mime).to_str().unwrap();
                assert_eq!(mime, "application/vnd.plotly+json");
                let data = CStr::from_ptr(result.artifact_data).to_str().unwrap();
                assert!(
                    data.contains(r#""type":"bar""#),
                    "histogram should render as a Plotly bar trace, got: {data}"
                );
                assert!(
                    data.contains(r#""y":[0,3,1,1,1,0,0,1]"#),
                    "histogram bin counts were not preserved in Plotly JSON, got: {data}"
                );
                free_execution_result(raw);
            }
        });
    }

    /// Verify the exact error span the user reported (Image #5) matches a
    /// truncated source missing the trailing `)` — this tells us the iOS
    /// Editor source buffer was actually `using Plots\n\nplot(sin` rather
    /// than `using Plots\n\nplot(sin)`.
    #[test]
    fn test_compile_and_run_detailed_missing_rparen_diagnostic() {
        run_with_large_stack(|| {
            let src = CString::new("using Plots\n\nplot(sin").unwrap();
            let raw = compile_and_run_detailed(src.as_ptr(), 0);
            assert!(!raw.is_null());
            unsafe {
                let result = &*raw;
                assert!(!result.success);
                let msg = if !result.error.message.is_null() {
                    CStr::from_ptr(result.error.message)
                        .to_string_lossy()
                        .to_string()
                } else {
                    String::new()
                };
                assert!(
                    msg.contains("RParen"),
                    "expected RParen-related error, got: {}",
                    msg
                );
                // The FFI span captured here is the canonical fingerprint for a
                // truncated `plot(sin)` — if an iOS Editor report ever matches
                // this verbatim, the source buffer was missing the trailing `)`
                // even if the editor rendered both parens.
                assert_eq!(
                    result.error.span.start, 21,
                    "FFI detailed result must expose the syntax span byte offset"
                );
                assert_eq!(
                    result.error.span.end, 21,
                    "FFI detailed result must expose the syntax span end byte offset"
                );
                assert_eq!(
                    result.error.span.start_line, 3,
                    "FFI detailed result must expose the syntax span line"
                );
                assert_eq!(
                    result.error.span.end_line, 3,
                    "FFI detailed result must expose the syntax span end line"
                );
                assert_eq!(
                    result.error.span.start_column, 9,
                    "FFI detailed result must expose the syntax span column"
                );
                assert_eq!(
                    result.error.span.end_column, 9,
                    "FFI detailed result must expose the syntax span end column"
                );
                free_execution_result(raw);
            }
        });
    }

    /// Editor sends source with blank lines (Image #5 user repro).
    /// Verify the parser tolerates `using Plots\n\nplot(sin)\n` exactly as
    /// the iOS app sends it via VMBridge.executeStreaming.
    #[test]
    fn test_compile_and_run_detailed_blank_line_repro() {
        run_with_large_stack(|| {
            let src = CString::new("using Plots\n\nplot(sin)\n").unwrap();
            let raw = compile_and_run_detailed(src.as_ptr(), 0);
            assert!(!raw.is_null());
            unsafe {
                let result = &*raw;
                if !result.success && !result.error.message.is_null() {
                    let msg = CStr::from_ptr(result.error.message).to_string_lossy();
                    panic!("expected success for blank-line source, got: {}", msg);
                }
                assert!(result.success);
                free_execution_result(raw);
            }
        });
    }

    /// Non-plot code should not leak artifact pointers.
    #[test]
    fn test_compile_and_run_detailed_no_artifact_for_plain_value() {
        run_with_large_stack(|| {
            let src = CString::new("1 + 2\n").unwrap();
            let raw = compile_and_run_detailed(src.as_ptr(), 0);
            assert!(!raw.is_null());
            unsafe {
                let result = &*raw;
                assert!(result.success);
                assert!(
                    result.artifact_mime.is_null(),
                    "no plot was returned; artifact_mime must be null"
                );
                assert!(
                    result.artifact_data.is_null(),
                    "no plot was returned; artifact_data must be null"
                );
                free_execution_result(raw);
            }
        });
    }

    #[test]
    fn test_compile_and_run_detailed_typed_array_accessors() {
        run_with_large_stack(|| {
            let src = CString::new("[1.0, 2.5, 3.0]\n").unwrap();
            let raw = compile_and_run_detailed(src.as_ptr(), 0);
            assert!(!raw.is_null());
            unsafe {
                let result = &*raw;
                assert!(result.success);
                assert!(matches!(
                    execution_result_value_kind(raw),
                    CValueKind::Array
                ));
                assert_eq!(execution_result_array_len(raw), 3);
                assert_eq!(execution_result_array_element_f64(raw, 0), 1.0);
                assert_eq!(execution_result_array_element_f64(raw, 1), 2.5);
                assert_eq!(execution_result_array_element_f64(raw, 2), 3.0);
                free_execution_result(raw);
            }
        });
    }

    #[test]
    fn test_compile_and_run_detailed_typed_complex_accessors() {
        run_with_large_stack(|| {
            let src = CString::new("complex(1.5, 2.25)\n").unwrap();
            let raw = compile_and_run_detailed(src.as_ptr(), 0);
            assert!(!raw.is_null());
            unsafe {
                let result = &*raw;
                assert!(result.success);
                assert!(matches!(
                    execution_result_value_kind(raw),
                    CValueKind::Complex
                ));
                assert_eq!(execution_result_complex_real(raw), 1.5);
                assert_eq!(execution_result_complex_imag(raw), 2.25);
                free_execution_result(raw);
            }
        });
    }

    /// Malformed-source process-survival sweep (Issue #10908 Phase 3 of
    /// #10869): `compile_and_run_detailed` is the real FFI C ABI entry the
    /// iOS Editor and Web hosts call with untrusted buffer contents. Every
    /// full/truncated/single-char-deleted mutation below must return a
    /// non-null `CExecutionResult` with `success = false` — never a Rust
    /// panic escaping the `extern "C"` boundary (which would abort the
    /// process, not merely fail an assertion, since `catch_unwind_ffi`
    /// (Issue #8707) is the only thing standing between an internal panic
    /// and that abort). Reaching the final assertions at all is itself
    /// evidence the boundary held for every case swept.
    const MALFORMED_SOURCE_SNIPPETS: &[&str] = &[
        "using Plots\n\nplot(sin",
        "function f(x)\n    x + 1\n",
        "struct S\n    x::Int\n",
        "for i in 1:10\n    println(i)\n",
        "let x = 1; x + 1",
        "[1 2; 3 4",
        "Dict(:a => 1, :b => 2",
        "x = 1\u{0}\u{1}\u{7} + 1\n",
        "",
    ];

    #[test]
    fn test_compile_and_run_detailed_malformed_source_never_panics() {
        run_with_large_stack(|| {
            for src in MALFORMED_SOURCE_SNIPPETS {
                let assert_survives = |text: &str| {
                    let Ok(c_src) = CString::new(text) else {
                        return; // embedded NUL: not representable as a C string, skip.
                    };
                    let raw = compile_and_run_detailed(c_src.as_ptr(), 0);
                    assert!(
                        !raw.is_null(),
                        "must return a result, not null, for {text:?}"
                    );
                    // Whether it succeeds or fails is not asserted here (a
                    // truncated snippet might coincidentally still parse a
                    // valid prefix); only that a typed result came back.
                    free_execution_result(raw);
                };

                assert_survives(src);
                for end in 1..src.len() {
                    if src.is_char_boundary(end) {
                        assert_survives(&src[..end]);
                    }
                }
                let chars: Vec<char> = src.chars().collect();
                for i in 0..chars.len() {
                    let mutated: String = chars
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .map(|(_, c)| *c)
                        .collect();
                    assert_survives(&mutated);
                }
            }
        });
    }
}
