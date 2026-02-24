// Prevent accidental debug output in library code (Issue #2888).
// CLI binaries (bin/) may use eprintln!() for user-facing error messages.
#![deny(clippy::print_stderr)]

// Core modules
pub mod builtins;
pub mod cancel;
pub mod compile;
pub mod error;
pub mod include;
pub mod intrinsics;
pub mod ir;
pub mod julia;
pub use julia::{base, stdlib}; // Re-export for backwards compatibility
pub mod base_loader;
pub mod loader;
pub mod rng;
pub mod span;
pub mod types;
pub mod unicode;
pub mod vm;

// Parser module
pub mod parser;

// Pure Rust stdlib loader
pub mod stdlib_loader;

// Lowering: CST -> Core IR
pub mod lowering;

// Bytecode file format
pub mod bytecode;

// REPL session management
pub mod repl;

// AoT (Ahead-of-Time) compiler module
#[cfg(feature = "aot")]
pub mod aot;

// Pipeline: parse and lower Julia source
pub mod pipeline;
pub use pipeline::get_prelude_program;

// Rust API for programmatic use
pub mod api;
pub use api::{
    compile_and_run_auto_str, compile_and_run_str, compile_and_run_value, compile_to_ir_str,
    run_ir_json_str,
};

// FFI module (C ABI functions)
pub mod ffi;

// Re-export all FFI functions at crate root for backwards compatibility
pub use ffi::{
    // Basic FFI
    compile_and_run,
    compile_and_run_auto,
    // Detailed error FFI
    compile_and_run_detailed,
    compile_and_run_streaming,
    compile_and_run_with_output,
    compile_to_ir,
    // Error types
    free_execution_result,
    // REPL FFI
    free_repl_result,
    free_string,
    is_expression_complete,
    repl_session_eval,
    repl_session_free,
    repl_session_new,
    repl_session_reset,
    run_ir_json_f64,
    run_ir_json_f64_N_seed,
    run_ir_json_f_N_seed,
    split_expressions,
    // Demo
    subset_julia_vm_demo,
    vm_request_cancel,
    vm_reset_cancel,
    CError,
    CErrorKind,
    CExecutionResult,
    CREPLResult,
    CSpan,
    OutputCallback,
};

// Unicode FFI (non-WASM only)
#[cfg(not(target_arch = "wasm32"))]
pub use ffi::{unicode_completions, unicode_expand, unicode_lookup, unicode_reverse_lookup};
