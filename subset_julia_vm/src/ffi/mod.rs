//! FFI (Foreign Function Interface) module.
//!
//! This module provides C ABI functions for iOS and other native platforms.
//! These functions are exposed as `extern "C"` and can be called from Swift, C, etc.

mod basic;
mod demo;
mod detailed;
mod error;
mod format;
mod repl_ffi;
#[cfg(not(target_arch = "wasm32"))]
mod unicode_ffi;

// Re-export all FFI functions and types at the module level
pub use basic::{
    compile_and_run, compile_and_run_auto, compile_and_run_with_output, compile_to_ir, free_string,
    run_ir_json_f64, run_ir_json_f64_N_seed, run_ir_json_f_N_seed, vm_request_cancel,
    vm_reset_cancel,
};

pub use demo::subset_julia_vm_demo;

pub use detailed::{compile_and_run_detailed, compile_and_run_streaming, OutputCallback};

pub use error::{free_execution_result, CError, CErrorKind, CExecutionResult, CSpan};

pub use format::{format_struct_instance, format_value};

pub use repl_ffi::{
    free_repl_result, is_expression_complete, repl_session_eval, repl_session_free,
    repl_session_new, repl_session_reset, split_expressions, CREPLResult,
};

#[cfg(not(target_arch = "wasm32"))]
pub use unicode_ffi::{
    unicode_completions, unicode_expand, unicode_lookup, unicode_reverse_lookup,
};
