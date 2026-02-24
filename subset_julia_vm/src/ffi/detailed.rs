//! Detailed error FFI functions.
//!
//! These functions provide rich error information including source spans and hints.

// FFI functions intentionally take raw pointers and are called from C/Swift code.
// The caller is responsible for ensuring pointer validity.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::CStr;
use std::os::raw::c_char;

use crate::cancel;
use crate::compile::compile_with_cache;
use crate::pipeline::{parse_and_lower, PipelineError};
use crate::rng::StableRng;
use crate::vm::{Value, Vm};

use super::error::{syntax_error_span, CError, CExecutionResult, CSpan};

/// Output callback function type for streaming output.
/// Takes a context pointer and the output string (null-terminated C string).
pub type OutputCallback = extern "C" fn(context: *mut std::os::raw::c_void, output: *const c_char);

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
            let result_value = match &value {
                Value::I64(x) => *x as f64,
                Value::F64(x) => *x,
                Value::Nothing => 0.0,
                val if val.is_complex() => {
                    val.as_complex_parts().map(|(re, _)| re).unwrap_or(f64::NAN)
                }
                _ => f64::NAN,
            };
            let result = CExecutionResult::success(result_value, output);
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
            let result_value = match &value {
                Value::I64(x) => *x as f64,
                Value::F64(x) => *x,
                Value::Nothing => 0.0,
                val if val.is_complex() => {
                    val.as_complex_parts().map(|(re, _)| re).unwrap_or(f64::NAN)
                }
                _ => f64::NAN,
            };
            let result = CExecutionResult::success(result_value, output);
            Box::into_raw(Box::new(result))
        }
        Err(e) => {
            let output = vm.get_output().to_string();
            let result = CExecutionResult::failure(output, CError::runtime(format!("{}", e)));
            Box::into_raw(Box::new(result))
        }
    }
}
