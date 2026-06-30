//! Detailed error FFI functions.
//!
//! These functions provide rich error information including source spans and hints.

// FFI functions intentionally take raw pointers and are called from C/Swift code.
// The caller is responsible for ensuring pointer validity.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::CStr;
use std::os::raw::c_char;

use subset_julia_vm::cancel;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::pipeline::{parse_and_lower, PipelineError};
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;

use super::error::{syntax_error_span, CError, CExecutionResult, CSpan};

/// Output callback function type for streaming output.
/// Takes a context pointer and the output string (null-terminated C string).
pub type OutputCallback = extern "C" fn(context: *mut std::os::raw::c_void, output: *const c_char);

fn warm_start_compile_cache() {
    // Mirror the CLI warm-start overlap for native app entrypoints (Issue #8463).
    subset_julia_vm::compile::cache::begin_warm_start_prefetch();
    subset_julia_vm::compile::cache::warm_base_cache();
}

/// Compile and run with detailed error information.
/// Returns a heap-allocated CExecutionResult that must be freed with free_execution_result.
#[no_mangle]
pub extern "C" fn compile_and_run_detailed(
    src_ptr: *const c_char,
    seed: u64,
) -> *mut CExecutionResult {
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

    // Parse and lower using tree-sitter pipeline
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

    match vm.run() {
        Ok(value) => {
            let output = vm.get_output().to_string();
            let artifact =
                subset_julia_vm::plotting::try_value_to_artifact(&value, vm.get_struct_heap());
            let result = CExecutionResult::success_with_value(&value, vm.get_struct_heap(), output)
                .with_artifact(artifact);
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

    // Parse and lower using tree-sitter pipeline
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

    match vm.run() {
        Ok(value) => {
            let output = vm.get_output().to_string();
            let artifact =
                subset_julia_vm::plotting::try_value_to_artifact(&value, vm.get_struct_heap());
            let result = CExecutionResult::success_with_value(&value, vm.get_struct_heap(), output)
                .with_artifact(artifact);
            Box::into_raw(Box::new(result))
        }
        Err(e) => {
            let output = vm.get_output().to_string();
            let result = CExecutionResult::failure(output, CError::runtime(format!("{}", e)));
            Box::into_raw(Box::new(result))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{
        execution_result_array_element_f64, execution_result_array_len,
        execution_result_complex_imag, execution_result_complex_real, execution_result_value_kind,
        free_execution_result, CValueKind,
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
                // The error span captured here (start: 21, end: 21,
                // start_line: 3, end_line: 3, start_column: 9, end_column: 9)
                // is the canonical fingerprint for a truncated `plot(sin)` — if
                // an iOS Editor report ever matches this verbatim, the source
                // buffer was missing the trailing `)` even if the editor
                // rendered both parens.
                assert!(msg.contains("start: 21"), "diagnostic span moved: {}", msg);
                assert!(
                    msg.contains("start_column: 9"),
                    "diagnostic span moved: {}",
                    msg
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
}
